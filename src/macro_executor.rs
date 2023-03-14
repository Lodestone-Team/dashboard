use std::{
    collections::HashMap,
    fmt::Debug,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use color_eyre::eyre::Context;
use tokio::{
    runtime::Builder,
    sync::{broadcast, oneshot, Mutex},
    task::LocalSet,
};
use tracing::{debug, error};

use crate::{
    error::{Error, ErrorKind},
    events::{CausedBy, Event, EventInner, MacroEvent, MacroEventInner},
    types::InstanceUuid,
};

use color_eyre::eyre::eyre;

use std::pin::Pin;

use anyhow::bail;
use deno_ast::MediaType;
use deno_ast::ParseParams;
use deno_ast::SourceTextInfo;
use deno_core::resolve_import;
use deno_core::ModuleLoader;
use deno_core::ModuleSource;
use deno_core::ModuleSourceFuture;
use deno_core::ModuleSpecifier;
use deno_core::ModuleType;
use deno_core::ResolutionKind;
use deno_core::{anyhow, error::generic_error};

use futures::FutureExt;

pub trait MainWorkerGenerator: Send + Sync {
    fn generate(&self, args: Vec<String>, caused_by: CausedBy) -> deno_runtime::worker::MainWorker;
}
pub struct TypescriptModuleLoader {
    http: reqwest::Client,
}

impl Default for TypescriptModuleLoader {
    fn default() -> Self {
        Self {
            http: reqwest::Client::new(),
        }
    }
}

impl ModuleLoader for TypescriptModuleLoader {
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        _kind: ResolutionKind,
    ) -> Result<ModuleSpecifier, anyhow::Error> {
        Ok(resolve_import(specifier, referrer)?)
    }

    fn load(
        &self,
        module_specifier: &ModuleSpecifier,
        _maybe_referrer: Option<ModuleSpecifier>,
        _is_dyn_import: bool,
    ) -> Pin<Box<ModuleSourceFuture>> {
        let module_specifier = module_specifier.clone();
        let http = self.http.clone();
        async move {
            let (code, module_type, media_type, should_transpile) = match module_specifier
                .to_file_path()
            {
                Ok(path) => {
                    let media_type = MediaType::from(&path);
                    let (module_type, should_transpile) = match media_type {
                        MediaType::JavaScript | MediaType::Mjs | MediaType::Cjs => {
                            (ModuleType::JavaScript, false)
                        }
                        MediaType::Jsx => (ModuleType::JavaScript, true),
                        MediaType::TypeScript
                        | MediaType::Mts
                        | MediaType::Cts
                        | MediaType::Dts
                        | MediaType::Dmts
                        | MediaType::Dcts
                        | MediaType::Tsx => (ModuleType::JavaScript, true),
                        MediaType::Json => (ModuleType::Json, false),
                        _ => bail!("Unknown extension {:?}", path.extension()),
                    };

                    (
                        tokio::fs::read_to_string(&path).await?,
                        module_type,
                        media_type,
                        should_transpile,
                    )
                }
                Err(_) => {
                    if module_specifier.scheme() == "http" || module_specifier.scheme() == "https" {
                        let http_res = http.get(module_specifier.to_string()).send().await?;
                        if !http_res.status().is_success() {
                            bail!("Failed to fetch module: {module_specifier}");
                        }
                        let content_type = http_res
                            .headers()
                            .get("content-type")
                            .and_then(|ct| ct.to_str().ok())
                            .ok_or_else(|| generic_error("No content-type header"))?;
                        let media_type =
                            MediaType::from_content_type(&module_specifier, content_type);
                        let (module_type, should_transpile) = match media_type {
                            MediaType::JavaScript | MediaType::Mjs | MediaType::Cjs => {
                                (ModuleType::JavaScript, false)
                            }
                            MediaType::Jsx => (ModuleType::JavaScript, true),
                            MediaType::TypeScript
                            | MediaType::Mts
                            | MediaType::Cts
                            | MediaType::Dts
                            | MediaType::Dmts
                            | MediaType::Dcts
                            | MediaType::Tsx => (ModuleType::JavaScript, true),
                            MediaType::Json => (ModuleType::Json, false),
                            _ => bail!("Unknown content-type {:?}", content_type),
                        };
                        let code = http_res.text().await?;
                        (code, module_type, media_type, should_transpile)
                    } else {
                        bail!("Unsupported module specifier: {}", module_specifier);
                    }
                }
            };
            let code = if should_transpile {
                let parsed = deno_ast::parse_module(ParseParams {
                    specifier: module_specifier.to_string(),
                    text_info: SourceTextInfo::from_string(code),
                    media_type,
                    capture_tokens: false,
                    scope_analysis: false,
                    maybe_syntax: None,
                })?;
                parsed
                    .transpile(&Default::default())?
                    .text
                    .into_bytes()
                    .into_boxed_slice()
            } else {
                code.into_bytes().into_boxed_slice()
            };

            let module = ModuleSource {
                code,
                module_type,
                module_url_specified: module_specifier.to_string(),
                module_url_found: module_specifier.to_string(),
            };
            Ok(module)
        }
        .boxed_local()
    }
}

#[derive(Clone, Debug)]
pub struct MacroExecutor {
    macro_process_table: Arc<Mutex<HashMap<usize, deno_core::v8::IsolateHandle>>>,
    event_broadcaster: broadcast::Sender<Event>,
    next_process_id: Arc<AtomicUsize>,
}

impl MacroExecutor {
    pub fn new(event_broadcaster: broadcast::Sender<Event>) -> MacroExecutor {
        let process_table = Arc::new(Mutex::new(HashMap::new()));
        let process_id = Arc::new(AtomicUsize::new(0));
        MacroExecutor {
            macro_process_table: process_table,
            event_broadcaster,
            next_process_id: process_id,
        }
    }

    pub async fn spawn(
        &self,
        path_to_main_module: PathBuf,
        args: Vec<String>,
        caused_by: CausedBy,
        main_worker_generator: Box<dyn MainWorkerGenerator>,
        instance_uuid: Option<InstanceUuid>,
    ) -> Result<usize, Error> {
        let pid = self.next_process_id.fetch_add(1, Ordering::SeqCst);

        let rt = Builder::new_current_thread().enable_all().build().unwrap();
        std::thread::spawn({
            let process_table = self.macro_process_table.clone();
            let event_broadcaster = self.event_broadcaster.clone();
            move || {
                let local = LocalSet::new();
                local.spawn_local(async move {
                    let mut runtime = main_worker_generator.generate(args, caused_by);

                    let isolate_handle = runtime.js_runtime.v8_isolate().thread_safe_handle();

                    let main_module =
                        match deno_core::resolve_path(&path_to_main_module.to_string_lossy()) {
                            Ok(v) => v,
                            Err(e) => {
                                error!("Error resolving main module: {}", e);
                                return;
                            }
                        };
                    process_table.lock().await.insert(pid, isolate_handle);

                    let _ = event_broadcaster.send(
                        MacroEvent {
                            macro_pid: pid,
                            macro_event_inner: MacroEventInner::MacroStarted,
                            instance_uuid: instance_uuid.clone(),
                        }
                        .into(),
                    );

                    let _ = runtime
                        .execute_main_module(&main_module)
                        .await
                        .map_err(|e| {
                            println!("Error executing main module: {}", e);
                            e
                        });
                    let event_broadcaster = event_broadcaster.clone();

                    let _ = runtime.run_event_loop(false).await.map_err(|e| {
                        println!("Error while running event loop: {}", e);
                    });

                    let _ = event_broadcaster.send(
                        MacroEvent {
                            macro_pid: pid,
                            macro_event_inner: MacroEventInner::MacroStopped,
                            instance_uuid,
                        }
                        .into(),
                    );

                    // If the while loop returns, then all the LocalSpawner
                    // objects have been dropped.
                });

                // This will return once all senders are dropped and all
                // spawned tasks have returned.
                rt.block_on(local);
                debug!("MacroExecutor thread exited");
            }
        });

        // listen to event broadcaster for macro started event
        // and return the pid

        let rx = self.event_broadcaster.subscribe();

        let fut = async move {
            let mut rx = rx;
            loop {
                if let Ok(event) = rx.recv().await {
                    if let EventInner::MacroEvent(MacroEvent {
                        macro_pid,
                        macro_event_inner: MacroEventInner::MacroStarted,
                        ..
                    }) = event.event_inner
                    {
                        if macro_pid == pid {
                            return Ok(macro_pid);
                        }
                    }
                } else {
                    break Err(eyre!("Failed to receive macro started event"));
                }
            }
        };

        tokio::time::timeout(Duration::from_secs(1), fut)
            .await
            .context("Failed to spawn macro")??;
        Ok(pid)
    }

    /// abort a macro execution
    pub async fn abort_macro(&self, pid: &usize) -> Result<(), Error> {
        self.macro_process_table
            .lock()
            .await
            .get(pid)
            .ok_or_else(|| Error {
                kind: ErrorKind::NotFound,
                source: eyre!("Macro with pid {} not found", pid),
            })?
            .terminate_execution();
        Ok(())
    }
    /// wait for a macro to finish
    ///
    /// if timeout is None, wait forever
    ///
    /// if timeout is Some, wait for the specified amount of time
    ///
    /// returns true if the macro finished, false if the timeout was reached
    pub async fn wait_with_timeout(&self, taget_macro_pid: usize, timeout: Option<f64>) -> bool {
        let mut rx = self.event_broadcaster.subscribe();
        tokio::select! {
            _ = async {
                if let Some(timeout) = timeout {
                    tokio::time::sleep(Duration::from_secs_f64(timeout)).await;
                } else {
                    // create a future that never resolves
                    let (_tx, rx) = oneshot::channel::<()>();
                    let _ = rx.await;

                }
            } => {
                false
            }
            _ = {
                async {
                    loop {
                    let event = rx.recv().await.unwrap();
                    if let EventInner::MacroEvent(MacroEvent { macro_pid, macro_event_inner, .. }) = event.event_inner {
                        if taget_macro_pid == macro_pid {
                           if let MacroEventInner::MacroStopped = macro_event_inner {
                               break;
                           }
                        }
                    }
                }
            }} => {
                true
            }
        }
    }

    pub async fn get_macro_status(&self, pid: usize) -> Result<bool, Error> {
        let table = self.macro_process_table.lock().await;
        let handle = table.get(&pid).ok_or_else(|| Error {
            kind: ErrorKind::NotFound,
            source: eyre!("Macro with pid {} not found", pid),
        })?;
        Ok(!handle.is_execution_terminating())
    }
}

#[allow(unused_imports)]
mod tests {
    use std::path::Path;
    use std::pin::Pin;
    use std::{path::PathBuf, rc::Rc};

    use super::{MainWorkerGenerator, TypescriptModuleLoader};
    use crate::error::Error;
    use crate::events::CausedBy;
    use crate::types::InstanceUuid;
    use deno_core::error::generic_error;
    use deno_core::{anyhow, op, ModuleSpecifier};
    use deno_core::{resolve_import, ModuleLoader, ModuleSource, ModuleSourceFuture, ModuleType};
    use deno_runtime::permissions::{Permissions, PermissionsContainer};
    use futures::FutureExt;
    use serde_json::{json, Value};
    use tokio::sync::broadcast;
    struct BasicMainWorkerGenerator;

    impl MainWorkerGenerator for BasicMainWorkerGenerator {
        fn generate(
            &self,
            args: Vec<String>,
            caused_by: CausedBy,
        ) -> deno_runtime::worker::MainWorker {
            let bootstrap_options = deno_runtime::BootstrapOptions {
                args,
                ..Default::default()
            };

            let mut worker_options = deno_runtime::worker::WorkerOptions {
                bootstrap: bootstrap_options,
                ..Default::default()
            };

            worker_options.module_loader = Rc::new(TypescriptModuleLoader::default());
            let mut worker = deno_runtime::worker::MainWorker::bootstrap_from_options(
                deno_core::resolve_path(".").unwrap(),
                PermissionsContainer::new(Permissions::allow_all()),
                worker_options,
            );

            worker
                .execute_script(
                    "[dep_inject]",
                    format!(
                        "const caused_by = {};",
                        serde_json::to_string(&caused_by).unwrap()
                    )
                    .as_str(),
                )
                .unwrap();
            worker
        }
    }
    #[tokio::test]
    async fn basic_execution() {
        let (event_broadcaster, _) = broadcast::channel(10);
        // construct a macro executor
        let executor = super::MacroExecutor::new(event_broadcaster);

        // create a temp directory
        let temp_dir = tempdir::TempDir::new("macro_test").unwrap().into_path();

        // create a macro file

        let path_to_macro = temp_dir.join("test.ts");

        std::fs::write(
            &path_to_macro,
            r#"
            console.log("hello world");
            "#,
        )
        .unwrap();

        let basic_worker_generator = BasicMainWorkerGenerator;

        let pid = executor
            .spawn(
                path_to_macro,
                Vec::new(),
                CausedBy::Unknown,
                Box::new(basic_worker_generator),
                None,
            )
            .await
            .unwrap();
        assert!(executor.wait_with_timeout(pid, None).await);
    }

    #[tokio::test]
    async fn test_http_url() {
        let (event_broadcaster, _) = broadcast::channel(10);
        // construct a macro executor
        let executor = super::MacroExecutor::new(event_broadcaster);

        // create a temp directory
        let temp_dir = tempdir::TempDir::new("macro_test").unwrap().into_path();

        // create a macro file

        let path_to_macro = temp_dir.join("test.ts");

        std::fs::write(
            &path_to_macro,
            r#"
            import { readLines } from "https://deno.land/std@0.104.0/io/mod.ts";
            console.log("hello world");
            "#,
        )
        .unwrap();

        let basic_worker_generator = BasicMainWorkerGenerator;

        let pid = executor
            .spawn(
                path_to_macro,
                Vec::new(),
                CausedBy::Unknown,
                Box::new(basic_worker_generator),
                None,
            )
            .await
            .unwrap();
        assert!(executor.wait_with_timeout(pid, None).await);
    }
}