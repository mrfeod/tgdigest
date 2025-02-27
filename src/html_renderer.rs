use crate::context::AppContext;
use crate::util::*;

use std::collections::HashMap;
use std::fs::File;
use std::io::Write as _;
use std::path::PathBuf;
use tera::Tera;

fn format_number(
    value: &tera::Value,
    _: &HashMap<String, tera::Value>,
) -> std::result::Result<tera::Value, tera::Error> {
    let Some(number) = value.as_i64() else {
        return Err(tera::Error::msg("Argument is not a number"));
    };
    let thin_space = "\u{2009}";
    let formatted = number
        .to_string()
        .chars()
        .rev()
        .collect::<Vec<char>>()
        .chunks(3)
        .map(|chunk| chunk.iter().collect::<String>())
        .collect::<Vec<String>>()
        .join(thin_space)
        .chars()
        .rev()
        .collect::<String>();
    Ok(tera::Value::String(formatted))
}

pub struct HtmlRenderer {
    engine: Tera,
    output_dir: PathBuf,
}

impl HtmlRenderer {
    pub fn new(ctx: &AppContext) -> Result<HtmlRenderer> {
        let mut engine =
            Tera::new(format!("{}/**/*_template.html", ctx.input_dir.to_str().unwrap()).as_str())?;
        engine.autoescape_on(vec!["html"]);

        engine.register_filter("format_number", format_number);

        log::info!("Loaded templates:");
        for template in engine.get_template_names() {
            log::info!("{template}");
        }

        Ok(HtmlRenderer {
            engine,
            output_dir: ctx.output_dir.clone(),
        })
    }

    pub fn render(&self, template_name: &str, context: &tera::Context) -> Result<String> {
        self.engine
            .render(template_name, context)
            .map_err(Into::into)
    }

    pub fn render_to_file(&self, template_name: &str, context: &tera::Context) -> Result<PathBuf> {
        let rendered = self.render(template_name, context)?;

        let output_name = template_name
            .replace("_template", "")
            .replace("/", "_")
            .replace("\\", "_");

        let output_path = self.output_dir.join(output_name);

        let mut file = File::create(&output_path)?; // Use the cloned output_path
        file.write_all(rendered.as_bytes())?;
        Ok(output_path)
    }
}
