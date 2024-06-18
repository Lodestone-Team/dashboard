use crate::AppState;

async fn handle_docker_command(input_tokens: &[&str], app_state: AppState) {
    match input_tokens.first() {
        Some(&"add") => {
            if let Some(container_name) = input_tokens.get(1) {
                app_state
                    .docker_bridge
                    .add_to_watch_list(container_name.to_string())
                    .await;
                println!("Added {} to watch list", container_name);
            } else {
                println!("Please provide a container name to add");
            }
        }
        Some(&"remove") => {
            if let Some(container_name) = input_tokens.get(1) {
                app_state
                    .docker_bridge
                    .remove_from_watch_list(container_name.to_string())
                    .await;
                println!("Removed {} from watch list", container_name);
            } else {
                println!("Please provide a container name to remove");
            }
        }
        _ => {
            println!("Unknown command");
        }
    }
}

pub fn init(app_state: AppState) {
    // start a new thread to listen for commands in stdin
    tokio::spawn({
        let app_state = app_state;
        async move {
            loop {
                let app_state = app_state.clone();
                let mut input = String::new();
                let buf_size = std::io::stdin().read_line(&mut input).unwrap();
                if buf_size == 0 {
                    println!("EOF");
                    break;
                }
                let input_tokens = input.split_whitespace().collect::<Vec<&str>>();
                match input_tokens.first() {
                    Some(&"di") => {
                        // send input tokens without the first element
                        handle_docker_command(&input_tokens[1..], app_state).await;
                    }
                    _ => {
                        println!("Unknown command: {}", input);
                        continue;
                    }
                }
            }
        }
    });
}
