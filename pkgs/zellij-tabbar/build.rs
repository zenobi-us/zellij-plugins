fn main() {
    minijinja_embed::embed_templates!("src/template", &[".jinja"]);
}
