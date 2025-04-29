use anyhow::Result;
use clap::Parser;
use flate2::{write::GzEncoder, Compression};
use httparse::{Header, Request, Status};
use std::{io::Write, path::PathBuf, time::Duration};
use tokio::{
    fs,
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    time::timeout,
};

use http_server_starter_rust::*;

const PORT: u16 = 4221;
const READ_TIMEOUT_SECS: u64 = 5;

#[derive(Parser)]
struct Cli {
    /// Directory where the files are stored, as an absolute path
    #[arg(long, default_value = ".")]
    directory: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();
    let directory = args.directory;

    println!(
        "Starting server on port {PORT}, serving files from: {:?}",
        directory
    );

    let listener = TcpListener::bind(format!("127.0.0.1:{PORT}")).await?;

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                println!("Accepted connection from {}", addr);
                let directory = directory.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, directory).await {
                        eprintln!("Connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
            }
        }
    }
}

async fn handle_connection(mut stream: TcpStream, directory: PathBuf) -> Result<()> {
    let mut reader = BufReader::new(&mut stream);
    let mut keep_alive = true;

    loop {
        let buf = match timeout(Duration::from_secs(READ_TIMEOUT_SECS), reader.fill_buf()).await {
            Ok(Ok(buf)) => buf.to_vec(),
            Ok(Err(e)) => return Err(e.into()), // Error reading buffer
            Err(_) => break,                    // Timeout occurred
        };

        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut req = Request::new(&mut headers);
        let body = match req.parse(&buf)? {
            Status::Complete(size) => {
                reader.consume(buf.len());
                &buf[size..]
            }
            _ => continue, // Incomplete request, continue reading
        };

        // check for "Connection: close"
        if get_header_value(req.headers, "Connection")
            .is_some_and(|value| value.to_lowercase() == "close")
        {
            keep_alive = false;
        };

        let response = match handle_request(&req, &directory, body, keep_alive).await {
            Ok(res) => res,
            Err(e) => {
                eprintln!("Error handling request: {}", e);
                Response::internal_server_error().with_body(
                    "The server encountered an error and could not complete your request.",
                )
            }
        };

        reader.get_mut().write_all(&response.as_bytes()).await?;
        reader.get_mut().flush().await?;

        if !keep_alive {
            break;
        }
    }

    Ok(())
}

async fn handle_request(
    request: &Request<'_, '_>,
    directory: &PathBuf,
    body: &[u8],
    keep_alive: bool,
) -> Result<Response> {
    let path = request.path.unwrap_or_default();
    let method = request.method.unwrap_or_default();

    let response = match path {
        "/" => Response::ok(),
        p if p.starts_with("/echo") => echo_handler(path, request.headers)?,
        p if p.starts_with("/user-agent") => user_agent_handler(request.headers),
        p if p.starts_with("/files") => files_handler(path, method, directory, body).await?,
        _ => Response::not_found().with_body("Invalid path"),
    };

    Ok(if !keep_alive {
        response.connection_close()
    } else {
        response
    })
}

fn get_header_value(headers: &[Header], key: &str) -> Option<String> {
    for header in headers {
        if header.name.to_lowercase() == key.to_lowercase() {
            return Some(String::from_utf8_lossy(&header.value).to_string());
        }
    }
    None
}

fn echo_handler(path: &str, headers: &[Header]) -> Result<Response> {
    let payload = path.strip_prefix("/echo/").unwrap_or_default();
    let content_type = "text/plain";

    if let Some(encoding) = get_header_value(headers, "Accept-Encoding") {
        if encoding.contains("gzip") {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(payload.as_bytes())?;
            let compressed = encoder.finish()?;
            return Ok(Response::ok()
                .with_content_encoding("gzip")
                .with_content_type(content_type)
                .with_body(compressed));
        }
    }

    Ok(Response::ok()
        .with_content_type("text/plain")
        .with_body(payload))
}

fn user_agent_handler(headers: &[Header]) -> Response {
    match get_header_value(headers, "User-Agent") {
        Some(user_agent) => Response::ok()
            .with_content_type("text/plain")
            .with_body(user_agent),
        None => Response::bad_request().with_body("User-Agent header not found"),
    }
}

async fn files_handler(
    path: &str,
    method: &str,
    directory: &PathBuf,
    body: &[u8],
) -> Result<Response> {
    let file_name = path.strip_prefix("/files/").unwrap_or_default();
    if file_name.is_empty() {
        return Ok(Response::bad_request().with_body("File name is required"));
    }

    let file_path = directory.join(file_name);
    let response = match method {
        "GET" => match fs::read(file_path).await {
            Ok(content) => Response::ok()
                .with_content_type("application/octet-stream")
                .with_body(content),
            Err(_) => Response::not_found().with_body("File not found"),
        },
        "POST" => match fs::write(file_path, body).await {
            Ok(_) => Response::created().with_body("File created"),
            Err(_) => Response::internal_server_error().with_body("Failed to create file"),
        },
        _ => Response::method_not_allowed(),
    };
    Ok(response)
}
