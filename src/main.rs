use anyhow::Result;
use clap::Parser;
use flate2::{write::GzEncoder, Compression};
use httparse::{Header, Request, Status};
use std::{io::Write, path::Path};
use tokio::{
    fs,
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};

use http_server_starter_rust::*;

#[derive(Parser)]
struct Cli {
    /// Directory where the files are stored, as an absolute path
    #[arg(long, default_value = ".")]
    directory: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();
    let directory = args.directory;
    let listener = TcpListener::bind(format!("127.0.0.1:{PORT}")).await?;
    println!("Server listening on port {PORT}, using directory: {directory}");

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let directory = directory.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, directory).await {
                        eprintln!("Failed to handle connection: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
            }
        }
    }
}

async fn handle_connection(mut stream: TcpStream, directory: String) -> Result<()> {
    let mut buf_reader = BufReader::new(&mut stream);
    let buf = buf_reader.fill_buf().await?;

    let mut headers = [httparse::EMPTY_HEADER; 16];
    let mut req = Request::new(&mut headers);
    let res = req.parse(buf)?;
    let body = match res {
        Status::Complete(size) => buf[size..].to_vec(),
        _ => vec![],
    };

    println!("Received headers: {:?}", req.headers);
    println!("Received body: {:?}", String::from_utf8_lossy(&body));

    let path = req.path;
    let response = match path {
        None => HTTP_BAD_REQUEST.to_string(),
        Some(p) => {
            if p == "/" {
                format!("{HTTP_OK}\r\n")
            } else if p.starts_with("/echo") {
                match handle_echo_endpoint(p, req.headers) {
                    Ok(response) => response,
                    Err(_) => HTTP_INTERNAL_SERVER_ERROR.to_string(),
                }
            } else if p.starts_with("/user-agent") {
                handle_user_agent_endpoint(req.headers)
            } else if p.starts_with("/files") {
                handle_files_endpoint(req, &directory, &body).await?
            } else {
                HTTP_NOT_FOUND.to_string()
            }
        }
    };

    println!("Response: {}", response);

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

fn handle_echo_endpoint(path: &str, headers: &[Header]) -> Result<String> {
    let mut payload = path.rsplit('/').next().unwrap().to_string();
    let mut content_encoding = None;

    if let Some(encoding) = find_header_value(headers, "Accept-Encoding") {
        if encoding.contains("gzip") {
            content_encoding = Some("gzip");
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(payload.as_bytes())?;
            let compressed = encoder.finish()?;
            payload = unsafe { String::from_utf8_unchecked(compressed) };
        }
    }

    Ok(build_response(
        HTTP_OK,
        content_encoding,
        "text/plain",
        &payload,
    ))
}

fn handle_user_agent_endpoint(headers: &[Header]) -> String {
    match find_header_value(headers, "User-Agent") {
        Some(user_agent) => build_response(HTTP_OK, None, "text/plain", &user_agent),
        None => HTTP_BAD_REQUEST.to_string(),
    }
}

async fn handle_files_endpoint<'a>(
    req: Request<'a, 'a>,
    directory: &str,
    body: &[u8],
) -> Result<String> {
    let path = req.path.unwrap();
    let method = req.method.unwrap();
    let file_name = path.rsplit('/').next().unwrap();
    let file_path = Path::new(directory).join(file_name);

    let response = if method == "POST" {
        match fs::write(file_path, body).await {
            Ok(_) => HTTP_CREATED.to_string(),
            Err(_) => HTTP_INTERNAL_SERVER_ERROR.to_string(),
        }
    } else {
        match fs::read_to_string(file_path).await {
            Ok(content) => build_response(HTTP_OK, None, "application/octet-stream", &content),
            Err(_) => HTTP_NOT_FOUND.to_string(),
        }
    };
    Ok(response)
}

fn build_response(
    status: &str,
    content_encoding: Option<&str>,
    content_type: &str,
    body: &str,
) -> String {
    let content_encoding_header = if let Some(encoding) = content_encoding {
        format!("Content-Encoding: {}\r\n", encoding)
    } else {
        String::new()
    };
    format!(
        "{status}{content_encoding_header}Content-Type: {content_type}\r\nContent-Length:{}\r\n\r\n{}",
        body.len(),
        body,
    )
}
