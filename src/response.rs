#[derive(Debug)]
struct Header {
    name: String,
    value: String,
}

impl Header {
    fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug)]
pub struct Response {
    code: u16,
    reason: String,
    headers: Vec<Header>,
    body: Vec<u8>,
}

impl Response {
    pub fn new(code: u16, reason: impl Into<String>) -> Self {
        Self {
            code,
            reason: reason.into(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    pub fn ok() -> Self {
        Self::new(200, "OK")
    }

    pub fn created() -> Self {
        Self::new(201, "Created")
    }

    pub fn bad_request() -> Self {
        Self::new(400, "Bad Request")
    }

    pub fn not_found() -> Self {
        Self::new(404, "Not Found")
    }

    pub fn method_not_allowed() -> Self {
        Self::new(405, "Method Not Allowed")
    }

    pub fn internal_server_error() -> Self {
        Self::new(500, "Internal Server Error")
    }

    fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push(Header::new(name, value));
        self
    }

    pub fn with_content_type(self, content_type: impl Into<String>) -> Self {
        self.with_header("Content-Type", content_type)
    }

    pub fn with_content_encoding(self, encoding: impl Into<String>) -> Self {
        self.with_header("Content-Encoding", encoding)
    }

    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        let body_len = self.body.len();
        self.with_header("Content-Length", body_len.to_string())
    }

    pub fn connection_close(self) -> Self {
        self.with_header("Connection", "close")
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        let mut result = Vec::new();
        // Status line
        result.extend_from_slice(format!("HTTP/1.1 {} {}\r\n", self.code, self.reason).as_bytes());
        // Headers
        for header in &self.headers {
            result.extend_from_slice(format!("{}: {}\r\n", header.name, header.value).as_bytes());
        }
        // End of headers
        result.extend_from_slice(b"\r\n");
        // Body
        result.extend_from_slice(&self.body);
        result
    }
}
