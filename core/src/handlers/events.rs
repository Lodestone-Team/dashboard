use std::sync::Arc;

use axum::{
    extract::{ws::WebSocket, Path, Query, WebSocketUpgrade},
    response::Response,
    routing::get,
    Json, Router,
};
use axum_auth::AuthBearer;

use color_eyre::eyre::eyre;
use futures::{SinkExt, StreamExt};
use ringbuffer::{AllocRingBuffer, RingBufferExt};
use tracing::{debug, error};

use crate::output_types::ClientEvent;
use crate::types::InstanceUuid;
use crate::{
    auth::{user::UsersManager, user_id::UserId},
    db::read::search_events,
    error::{Error, ErrorKind},
    events::EventQuery,
};

use crate::{
    events::{Event, EventInner, UserEventInner},
    AppState,
};
use serde::Deserialize;
use tokio::sync::{broadcast::Receiver, RwLock};
use ts_rs::TS;

use super::util::parse_bearer_token;

#[derive(Deserialize, Clone, Debug, TS)]
pub struct EventQueryWrapper {
    filter: String,
}

pub async fn get_event_buffer(
    axum::extract::State(state): axum::extract::State<AppState>,
    AuthBearer(token): AuthBearer,
    query: Query<EventQueryWrapper>,
) -> Result<Json<Vec<Event>>, Error> {
    // deserialize query
    let query: EventQuery = serde_json::from_str(&query.filter).map_err(|e| {
        error!("Error deserializing event query: {}", e);
        Error {
            kind: ErrorKind::BadRequest,
            source: e.into(),
        }
    })?;
    let requester = state
        .users_manager
        .read()
        .await
        .try_auth(&token)
        .ok_or_else(|| Error {
            kind: ErrorKind::Unauthorized,
            source: eyre!("Token error"),
        })?;
    Ok(Json(
        state
            .events_buffer
            .lock()
            .await
            .iter()
            .filter(|event| {
                query.filter(ClientEvent::from(*event)) && requester.can_view_event(*event)
            })
            .cloned()
            .collect(),
    ))
}

// TODO implement me
pub async fn get_event_search(
    axum::extract::State(state): axum::extract::State<AppState>,
    AuthBearer(token): AuthBearer,
    query: Query<EventQueryWrapper>,
) -> Result<Json<Vec<ClientEvent>>, Error> {
    // deserialize query
    let query: EventQuery = serde_json::from_str(&query.filter).map_err(|e| {
        error!("Error deserializing event query: {}", e);
        Error {
            kind: ErrorKind::BadRequest,
            source: e.into(),
        }
    })?;
    let _requester = state
        .users_manager
        .read()
        .await
        .try_auth(&token)
        .ok_or_else(|| Error {
            kind: ErrorKind::Unauthorized,
            source: eyre!("Token error"),
        })?;
    search_events(&state.sqlite_pool, query).await.map(Json)
}

pub async fn get_console_buffer(
    axum::extract::State(state): axum::extract::State<AppState>,
    AuthBearer(token): AuthBearer,
    Path(uuid): Path<InstanceUuid>,
) -> Result<Json<Vec<Event>>, Error> {
    let requester = state
        .users_manager
        .read()
        .await
        .try_auth(&token)
        .ok_or_else(|| Error {
            kind: ErrorKind::Unauthorized,
            source: eyre!("Token error"),
        })?;
    Ok(Json(
        state
            .console_out_buffer
            .lock()
            .await
            .get(&uuid)
            .unwrap_or(&AllocRingBuffer::new())
            .iter()
            .filter(|event| match &event.event_inner {
                EventInner::InstanceEvent(instance_event) => {
                    (instance_event.instance_uuid == uuid || uuid == "all")
                        && requester.can_view_event(event)
                }
                _ => false,
            })
            .cloned()
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct WebsocketQuery {
    token: String,
}

pub async fn event_stream(
    ws: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<AppState>,
    query: Query<EventQueryWrapper>,
) -> Result<Response, Error> {
    let query: EventQuery = serde_json::from_str(query.filter.as_str()).map_err(|e| {
        error!("Error deserializing event query: {}", e);
        Error {
            kind: ErrorKind::BadRequest,
            source: e.into(),
        }
    })?;
    let token = query.bearer_token.clone().ok_or(Error {
        kind: ErrorKind::BadRequest,
        source: eyre!("Missing token"),
    })?;

    let user = state
        .users_manager
        .read()
        .await
        .try_auth(&token)
        .ok_or_else(|| Error {
            kind: ErrorKind::Unauthorized,
            source: eyre!("Token error"),
        })?;
    let event_receiver = state.event_broadcaster.subscribe();

    Ok(ws.on_upgrade(move |socket| {
        event_stream_ws(socket, event_receiver, query, user.uid, state.users_manager)
    }))
}

async fn event_stream_ws(
    stream: WebSocket,
    mut event_receiver: Receiver<Event>,
    query: EventQuery,
    uid: UserId,
    users_manager: Arc<RwLock<UsersManager>>,
) {
    let (mut sender, mut receiver) = stream.split();
    loop {
        tokio::select! {
            Ok(event) = event_receiver.recv() => {
                if event.is_event_console_message() {
                    continue;
                }
                let user = match users_manager.read().await.get_user(&uid) {
                    Some(user) => user,
                    None => {
                        break;
                    }
                };
                if query.filter(ClientEvent::from(event.clone())) && user.can_view_event(&event) {
                    if let Err(e) = sender.send(axum::extract::ws::Message::Text(serde_json::to_string(&event).unwrap())).await {
                        error!("Error sending event to websocket: {}", e);
                        break;
                    }
                }
            }
            Some(Ok(ws_msg)) = receiver.next() => {
                match sender.send(ws_msg).await {
                    Ok(_) => debug!("Replied to ping"),
                    Err(_) => {debug!("Websocket disconnected"); break},
                };
            }
        }
    }
}

pub async fn console_stream(
    ws: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<AppState>,
    query: Query<WebsocketQuery>,
    Path(uuid): Path<InstanceUuid>,
) -> Result<Response, Error> {
    let users_manager = state.users_manager.read().await;

    let user = parse_bearer_token(query.token.as_str())
        .and_then(|token| users_manager.try_auth(&token))
        .ok_or_else(|| Error {
            kind: ErrorKind::Unauthorized,
            source: eyre!("Token error"),
        })?;
    drop(users_manager);
    let event_receiver = state.event_broadcaster.subscribe();

    Ok(ws.on_upgrade(move |socket| {
        console_stream_ws(socket, event_receiver, user.uid, uuid, state.users_manager)
    }))
}

async fn console_stream_ws(
    stream: WebSocket,
    mut event_receiver: Receiver<Event>,
    uid: UserId,
    uuid: InstanceUuid,
    users_manager: Arc<RwLock<UsersManager>>,
) {
    let (mut sender, mut receiver) = stream.split();
    loop {
        tokio::select! {
            Ok(event) = event_receiver.recv() => {
                match &event.event_inner {
                    EventInner::InstanceEvent(instance_event) => {
                        let user = match users_manager.read().await.get_user(&uid) {
                            Some(user) => user,
                            None => break,
                        };
                        if event.is_event_console_message() && (instance_event.instance_uuid == uuid || uuid == "all")
                            && user.can_view_event(&event)
                        {
                            if let Err(e) = sender
                                .send(axum::extract::ws::Message::Text(
                                    serde_json::to_string(&event).unwrap(),
                                ))
                                .await
                            {
                                error!("Failed to send event: {}", e);
                                break;
                            }
                        }
                    }
                    EventInner::UserEvent(user_event) => {
                        match user_event.user_event_inner {
                            UserEventInner::UserLoggedOut | UserEventInner::UserDeleted => {
                                if user_event.user_id == uid {
                                    break;
                                }
                            },
                            _ => {}
                        }
                    },
                    EventInner::MacroEvent(_) => continue,
                    EventInner::ProgressionEvent(_) => continue,
                    EventInner::FSEvent(_) => continue,
                    EventInner::PlayitggRunnerEvent(_) => continue,
                }
            }
            Some(Ok(ws_msg)) = receiver.next() => {
                match sender.send(ws_msg).await {
                    Ok(_) => debug!("Replied to ping"),
                    Err(_) => break,
                };
            }
        }
    }
}

pub async fn clear_console_buffer(
    axum::extract::State(state): axum::extract::State<AppState>,
    AuthBearer(token): AuthBearer,
    Path(uuid): Path<InstanceUuid>,
) -> Result<Json<()>, Error> {
    let requester = state
        .users_manager
        .read()
        .await
        .try_auth(&token)
        .ok_or_else(|| Error {
            kind: ErrorKind::Unauthorized,
            source: eyre!("Token error"),
        })?;
    if !requester.can_perform_action(&UserAction::ClearConsoleBuffer(uuid.clone())) {
        return Err(Error {
            kind: ErrorKind::PermissionDenied,
            source: eyre!("You don't have permission to clear the console buffer"),
        });
    }
    state.clear_console_buffer(&uuid).await;
    Ok(Json(()))
}

pub fn get_events_routes(state: AppState) -> Router {
    Router::new()
        .route("/events/:uuid/stream", get(event_stream))
        .route("/events/:uuid/buffer", get(get_event_buffer))
        .route("/events/search", get(get_event_search))
        .route("/instance/:uuid/console/stream", get(console_stream))
        .route("/instance/:uuid/console/buffer", get(get_console_buffer))
        .route("/instance/:uuid/console/clear", get(clear_console_buffer))
        .with_state(state)
}
