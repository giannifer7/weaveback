# Serve Static Files

Static-file path safety, MIME detection, cache headers, and file responses.

## Static file serving

`content_type` maps a file extension to a MIME type.  `safe_path` sanitises a
URL path to a filesystem path under `html_dir`, rejecting `..` components and
appending `index.html` for directory requests.

```rust
// <[serve-static]>=
fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css")  => "text/css; charset=utf-8",
        Some("js")   => "application/javascript; charset=utf-8",
        Some("svg")  => "image/svg+xml",
        Some("png")  => "image/png",
        Some("ico")  => "image/x-icon",
        Some("json") => "application/json",
        _            => "application/octet-stream",
    }
}

fn safe_path(html_dir: &Path, url_path: &str) -> Option<PathBuf> {
    let rel = url_path.trim_start_matches('/');
    if rel.split('/').any(|c| c == "..") {
        return None;
    }
    let path = html_dir.join(rel);
    if path.is_dir() {
        let idx = path.join("index.html");
        if idx.exists() { Some(idx) } else { None }
    } else if path.exists() {
        Some(path)
    } else {
        None
    }
}

fn serve_static(request: Request, url: &str, html_dir: &Path) {
    let url_path = url.split('?').next().unwrap_or(url);

    // Redirect bare "/" to "/docs/index.html" so the browser's base URL is
    // correct and relative asset paths inside the HTML resolve properly.
    if url_path == "/" {
        let _ = request.respond(
            Response::from_string("")
                .with_status_code(302)
                .with_header(Header::from_bytes("Location", "/docs/index.html").unwrap()),
        );
        return;
    }

    match safe_path(html_dir, url_path) {
        None => {
            let _ = request.respond(Response::from_string("404 Not Found").with_status_code(404));
        }
        Some(path) => {
            let ct = content_type(&path);
            // ETag = "<mtime_secs>-<file_size>" — cheap, correct for a local dev server.
            let etag: Option<String> = std::fs::metadata(&path).ok().and_then(|m| {
                let secs = m.modified().ok()?
                    .duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
                Some(format!("\"{}-{}\"", secs, m.len()))
            });
            // Respond 304 if the client already has this version.
            if let Some(ref tag) = etag {
                let matched = request.headers().iter()
                    .find(|h| h.field.equiv("If-None-Match"))
                    .map(|h| h.value.as_str() == tag.as_str())
                    .unwrap_or(false);
                if matched {
                    let _ = request.respond(
                        Response::from_string("").with_status_code(304)
                    );
                    return;
                }
            }
            // JS/CSS/images: allow the browser to cache for 5 minutes.
            // HTML: always revalidate (but ETag avoids re-transfer when unchanged).
            let cache_ctrl = match path.extension().and_then(|e| e.to_str()) {
                Some("js") | Some("css") | Some("png") | Some("svg") | Some("ico") => "max-age=300",
                _ => "no-cache",
            };
            match std::fs::read(&path) {
                Ok(bytes) => {
                    let mut response = Response::from_data(bytes)
                        .with_header(Header::from_bytes("Content-Type", ct).unwrap())
                        .with_header(Header::from_bytes("Cache-Control", cache_ctrl).unwrap());
                    if let Some(tag) = etag {
                        response = response
                            .with_header(Header::from_bytes("ETag", tag).unwrap());
                    }
                    let _ = request.respond(response);
                }
                Err(_) => {
                    let _ = request.respond(
                        Response::from_string("500 Internal Server Error")
                            .with_status_code(500)
                    );
                }
            }
        }
    }
}
// @
```

