use async_std::{
    io::BufReader,
    net::{TcpListener, TcpStream},
    prelude::*,
};
use flate2::{write::GzEncoder, Compression};
use futures::{stream::StreamExt, AsyncBufReadExt};
use httparse::{Header, Request};
use std::{env, fs, io::Write};

static PORT: u16 = 4221;

#[async_std::main]
async fn main() {
    let directory = parse_args_for_directory();
    let listener = TcpListener::bind(format!("127.0.0.1:{PORT}"))
        .await
        .unwrap();
    println!("Server listening on port {PORT}, using directory: {directory}");

    listener
        .incoming()
        .for_each_concurrent(/* limit */ None, |tcpstream| {
            let directory = directory.clone();
            async move {
                match tcpstream {
                    Ok(tcpstream) => {
                        handle_connection(tcpstream, directory)
                            .await
                            .unwrap_or_else(|e| {
                                eprintln!("Failed to handle connection: {}", e);
                            });
                    }
                    Err(e) => {
                        eprintln!("Failed to establish connection: {}", e);
                    }
                }
            }
        })
        .await;
}

fn parse_args_for_directory() -> String {
    let args: Vec<String> = env::args().collect();
    let directory = if args.len() > 2 && args[1] == "--directory" {
        args[2].clone()
    } else {
        ".".to_string()
    };
    directory
}

async fn handle_connection(mut stream: TcpStream, directory: String) -> Result<(), std::io::Error> {
    let mut buf_reader = BufReader::new(&mut stream);
    let buf = buf_reader.fill_buf().await?;
    let mut headers = [httparse::EMPTY_HEADER; 16];
    let mut req = Request::new(&mut headers);
    let res = req.parse(buf).unwrap();
    let body = buf[res.unwrap()..].to_vec();

    println!("Headers: {:#?}", req.headers);
    println!("Body: {:#?}", String::from_utf8_lossy(&body));

    let path = req.path;
    let response = match path {
        None => "HTTP/1.1 400 Bad Request\r\n\r\n".to_string(),
        Some(p) => {
            if p == "/" {
                "HTTP/1.1 200 OK\r\n\r\n".to_string()
            } else if p.starts_with("/echo") {
                match handle_echo_endpoint(p, req.headers) {
                    Ok(response) => response,
                    Err(_) => "HTTP/1.1 500 Internal Server Error\r\n\r\n".to_string(),
                }
            } else if p.starts_with("/user-agent") {
                handle_user_agent_endpoint(req.headers)
            } else if p.starts_with("/files") {
                handle_files_endpoint(req, directory, body)
            } else {
                "HTTP/1.1 404 Not Found\r\n\r\n".to_string()
            }
        }
    };

    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

fn find_header_value(headers: &[Header], key: &str) -> Option<String> {
    for header in headers {
        if header.name.to_lowercase() == key.to_lowercase() {
            return Some(String::from_utf8_lossy(header.value).to_string());
        }
    }
    None
}

fn handle_echo_endpoint(path: &str, headers: &[Header]) -> Result<String, std::io::Error> {
    let mut string = path.split('/').last().unwrap().to_string();
    let mut response = "HTTP/1.1 200 OK\r\n".to_string();
    let mut compressed: Option<Vec<u8>> = None;

    if let Some(encoding) = find_header_value(headers, "Accept-Encoding") {
        if encoding.contains("gzip") {
            response.push_str("Content-Encoding: gzip\r\n");
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(string.as_bytes()).map_err(|e| e)?;
            compressed = Some(encoder.finish().map_err(|e| e)?);
        }
    }

    if let Some(c) = compressed {
        unsafe { string = String::from_utf8_unchecked(c) }
    }
    response.push_str(&format!(
        "Content-Type: text/plain\r\nContent-Length:{}\r\n\r\n{}",
        string.len(),
        string
    ));
    Ok(response)
}

fn handle_user_agent_endpoint(headers: &[Header]) -> String {
    match find_header_value(headers, "User-Agent") {
        Some(user_agent) => format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length:{}\r\n\r\n{}",
            user_agent.len(),
            user_agent
        ),
        None => "HTTP/1.1 400 Bad Request\r\n\r\n".to_string(),
    }
}

fn handle_files_endpoint(req: Request, directory: String, body: Vec<u8>) -> String {
    let path = req.path.unwrap();
    let method = req.method.unwrap();
    let file_name = path.split('/').last().unwrap();
    let file_path = format!("{}/{}", directory, file_name);
    let response: String;

    if method == "POST" {
        let res = fs::write(file_path, body);
        response = match res {
            Ok(_) => "HTTP/1.1 201 Created\r\n\r\n".to_string(),
            Err(_) => "HTTP/1.1 500 Internal Server Error\r\n\r\n".to_string(),
        };
    } else {
        let file_content = fs::read_to_string(file_path);
        response = match file_content {
            Ok(content) => format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length:{}\r\n\r\n{}",
                content.len(),
                content
            ),
            Err(_) => "HTTP/1.1 404 Not Found\r\n\r\n".to_string(),
        };
    }
    response
}
