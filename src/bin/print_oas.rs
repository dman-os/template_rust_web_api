use deps::*;

use template_rust_web_api::*;

fn main() {
    println!(
        "{}",
        <ApiDoc as utoipa::OpenApi>::openapi()
            .to_pretty_json()
            .unwrap()
    );
}
