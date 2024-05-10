use async_std::{io::BufReader, net::TcpListener, net::TcpStream, prelude::*};
use futures::future;
use futures::stream::StreamExt;
use std::collections::HashMap;

#[async_std::main]
async fn main() {
    println!("Logs from your program will appear here!");

    let listener = TcpListener::bind("127.0.0.1:4221").await.unwrap();

    listener
        .incoming()
        .for_each_concurrent(/* limit */ None, |tcpstream| async move {
            let tcpstream = tcpstream.unwrap();
            handle_connection(tcpstream).await;
        })
        .await;
}

async fn handle_connection(mut stream: TcpStream) {
    let buf_reader = BufReader::new(&mut stream);
    let http_request: Vec<_> = buf_reader
        .lines()
        .map(|result| result.unwrap())
        .take_while(|line| future::ready(!line.is_empty()))
        .collect()
        .await;

    println!("Request: {:#?}", http_request);

    let start_line = http_request.first().unwrap();
    let path = start_line.split_whitespace().nth(1).unwrap();
    let headers: HashMap<&str, &str> = http_request[1..]
        .iter()
        .map(|line| {
            let mut parts = line.splitn(2, ": ");
            (parts.next().unwrap(), parts.next().unwrap())
        })
        .collect();

    let response = match path {
        "/" => "HTTP/1.1 200 OK\r\n\r\n".to_string(),
        p if p.starts_with("/echo") => handle_echo_endpoint(p),
        p if p.starts_with("/user-agent") => {
            handle_user_agent_endpoint(headers.get("User-Agent").unwrap())
        }
        _ => "HTTP/1.1 404 Not Found\r\n\r\n".to_string(),
    };

    stream.write_all(response.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();
}

fn handle_echo_endpoint(path: &str) -> String {
    let string = path.split('/').last().unwrap();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length:{}\r\n\r\n{}",
        string.len(),
        string
    );
    response
}

fn handle_user_agent_endpoint(user_agent: &str) -> String {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length:{}\r\n\r\n{}",
        user_agent.len(),
        user_agent
    );
    response
}
