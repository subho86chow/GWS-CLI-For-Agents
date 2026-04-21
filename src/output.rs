use serde::Serialize;
use std::process;

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

#[derive(Serialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

pub fn print_json<T: Serialize>(value: T) {
    match serde_json::to_string_pretty(&value) {
        Ok(s) => println!("{}", s),
        Err(e) => {
            print_error("SERIALIZATION_ERROR", &e.to_string(), None);
            process::exit(1);
        }
    }
}

pub fn print_error(code: &str, message: &str, suggestion: Option<&str>) {
    let err = ErrorResponse {
        error: ErrorDetail {
            code: code.to_string(),
            message: message.to_string(),
            suggestion: suggestion.map(|s| s.to_string()),
        },
    };
    eprintln!("{}", serde_json::to_string_pretty(&err).unwrap_or_else(|_| {
        format!(
            r#"{{"error":{{"code":"{}","message":"{}"}}}}"#,
            code, message
        )
    }));
}

pub fn exit_with_error(code: &str, message: &str, suggestion: Option<&str>) -> ! {
    print_error(code, message, suggestion);
    let code_num = match code {
        "AUTH_ERROR" | "UNAUTHENTICATED" => 2,
        "PERMISSION_DENIED" => 3,
        _ => 1,
    };
    process::exit(code_num);
}
