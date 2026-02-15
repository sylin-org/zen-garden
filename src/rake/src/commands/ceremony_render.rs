//! Async HTTP ceremony render loop for garden-rake.
//!
//! This is the zen-garden equivalent of koi's `ceremony_cli` module.
//! It drives a ceremony hosted on Moss via HTTP POST and renders the
//! interactive flow in the terminal.
//!
//! The render loop is a dumb client — it displays whatever the server
//! sends (messages, prompts) and collects user input. All branching,
//! validation, and domain logic live on the server.

use colored::Colorize;
use koi_common::ceremony::{
    CeremonyRequest, CeremonyResponse, InputType, Message, MessageKind, Prompt, QrFormat,
    RenderHints,
};

// ── Box drawing ─────────────────────────────────────────────────────

fn visible_width(s: &str) -> usize {
    let mut width = 0usize;
    let mut in_escape = false;
    for ch in s.chars() {
        if in_escape {
            if ch == 'm' {
                in_escape = false;
            }
        } else if ch == '\x1b' {
            in_escape = true;
        } else {
            width += 1;
        }
    }
    width
}

fn pad_visible(s: &str, target: usize) -> String {
    let vw = visible_width(s);
    if vw >= target {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(target - vw))
    }
}

fn print_box(indent: &str, title: Option<&str>, lines: &[String]) {
    let max_content = lines.iter().map(|l| visible_width(l)).max().unwrap_or(0);
    let title_width = title.map(|t| visible_width(t) + 6).unwrap_or(0);
    let inner = max_content.max(title_width).max(20) + 2;

    if let Some(t) = title {
        let label = format!("── {t} ");
        let label_vw = visible_width(&label);
        let remaining = if inner + 2 > label_vw {
            inner + 2 - label_vw
        } else {
            1
        };
        println!("{indent}╭{label}{}╮", "─".repeat(remaining));
    } else {
        println!("{indent}╭{}╮", "─".repeat(inner + 2));
    }

    for line in lines {
        let padded = pad_visible(line, inner);
        println!("{indent}│ {padded} │");
    }

    println!("{indent}╰{}╯", "─".repeat(inner + 2));
}

// ── Prompt helper ───────────────────────────────────────────────────

fn prompt_line(prompt: &str) -> anyhow::Result<String> {
    use std::io::Write;
    print!("{prompt}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(line.trim_end().to_string())
}

// ── Public API ──────────────────────────────────────────────────────

/// Drive a ceremony over HTTP and render the interactive flow.
///
/// This is the zen-garden render loop. It:
/// 1. POSTs a CeremonyRequest to the Moss ceremony endpoint.
/// 2. Renders messages and prompts from the response.
/// 3. Collects user input.
/// 4. Repeats until the ceremony completes.
///
/// Returns the `result_data` bag from the final response.
pub async fn run_ceremony_http(
    client: &reqwest::Client,
    ceremony_url: &str,
    ceremony_type: &str,
    initial_data: serde_json::Map<String, serde_json::Value>,
) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
    let render_hints = RenderHints {
        qr: Some(QrFormat::Utf8),
    };

    // First request — start the ceremony
    let mut request = CeremonyRequest {
        session_id: None,
        ceremony: Some(ceremony_type.into()),
        data: initial_data,
        render: Some(render_hints.clone()),
    };

    loop {
        // POST to Moss ceremony endpoint
        let http_resp = client
            .post(ceremony_url)
            .json(&request)
            .send()
            .await?;

        if !http_resp.status().is_success() {
            let status = http_resp.status();
            let body = http_resp.text().await.unwrap_or_default();
            anyhow::bail!("Ceremony request failed: {status} {body}");
        }

        let response: CeremonyResponse = http_resp.json().await?;

        // Render messages
        render_response(&response)?;

        // Check completion
        if response.complete {
            if let Some(err) = &response.error {
                anyhow::bail!("{err}");
            }
            return Ok(response.result_data.unwrap_or_default());
        }

        // Non-fatal error already rendered by render_response

        // Collect input from prompts
        let data = collect_prompts(&response.prompts)?;

        // Build next request
        request = CeremonyRequest {
            session_id: Some(response.session_id),
            ceremony: None,
            data,
            render: Some(render_hints.clone()),
        };
    }
}

// ── Rendering ───────────────────────────────────────────────────────

fn render_response(response: &CeremonyResponse) -> anyhow::Result<()> {
    // Show error if present
    if let Some(err) = &response.error {
        println!("\n  {} {}", "✗".red(), err.red());
    }

    // Render messages
    for msg in &response.messages {
        render_message(msg);
    }

    Ok(())
}

fn render_message(msg: &Message) {
    println!();
    match msg.kind {
        MessageKind::Info => {
            if msg.title.starts_with('⚠') {
                // Warning-style info
                println!("  {}", msg.title.yellow());
                for line in msg.content.lines() {
                    println!("  {}", line.yellow());
                }
            } else {
                println!("  {}", msg.title.dimmed());
                for line in msg.content.lines() {
                    println!("  {}", line.dimmed());
                }
            }
        }
        MessageKind::QrCode => {
            println!("  {}\n", msg.title);
            if msg.content.contains('█') || msg.content.contains('▄') {
                // UTF-8 QR art — print as-is
                println!("{}", msg.content);
            } else if msg.content.starts_with("otpauth://") {
                // URI-only mode
                println!("  {}\n", msg.content.cyan().bold());
            } else {
                // Base64 PNG
                println!("  {}", "(QR image available as base64 PNG)".dimmed());
            }
        }
        MessageKind::Summary => {
            let mut lines: Vec<String> = Vec::new();
            lines.push(String::new());
            for line in msg.content.lines() {
                lines.push(line.to_string());
            }
            lines.push(String::new());
            let title_str = msg.title.green().to_string();
            print_box("  ", Some(&title_str), &lines);
        }
        MessageKind::Error => {
            println!("  {} {}", "✗".red(), msg.title.red());
            for line in msg.content.lines() {
                println!("    {}", line.red());
            }
        }
    }
}

// ── Input collection ────────────────────────────────────────────────

fn collect_prompts(
    prompts: &[Prompt],
) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
    let mut data = serde_json::Map::new();

    for prompt in prompts {
        let value = collect_single_prompt(prompt)?;
        data.insert(prompt.key.clone(), serde_json::Value::String(value));
    }

    Ok(data)
}

fn collect_single_prompt(prompt: &Prompt) -> anyhow::Result<String> {
    match prompt.input_type {
        InputType::SelectOne => collect_select_one(prompt),
        InputType::Text => collect_text(prompt),
        InputType::Secret => collect_secret(prompt),
        InputType::SecretConfirm => collect_secret_confirm(prompt),
        InputType::Code => collect_code(prompt),
        InputType::Entropy => collect_entropy(prompt),
        InputType::Fido2 => {
            anyhow::bail!("FIDO2 hardware key input is not yet supported in this CLI.");
        }
        InputType::SelectMany => collect_text(prompt),
    }
}

fn collect_select_one(prompt: &Prompt) -> anyhow::Result<String> {
    println!();
    println!("  {}\n", prompt.prompt);

    for (i, opt) in prompt.options.iter().enumerate() {
        let num = i + 1;
        let default_marker = if num == 1 { " (default)" } else { "" };
        println!(
            "  [{}] {}{}",
            if num == 1 {
                num.to_string().cyan().to_string()
            } else {
                num.to_string()
            },
            opt.label,
            default_marker.dimmed()
        );
        if let Some(desc) = &opt.description {
            for line in textwrap_simple(desc, 60) {
                println!("      {}", line.dimmed());
            }
        }
        println!();
    }

    loop {
        let line = prompt_line(&format!(
            "  Choose [1-{}, {}=1, esc={}]: ",
            prompt.options.len(),
            "Enter".cyan(),
            "cancel".dimmed(),
        ))?;

        let trimmed = line.trim().to_ascii_lowercase();

        if trimmed == "esc" {
            anyhow::bail!("Canceled. No changes made.");
        }

        if trimmed.is_empty() {
            let value = &prompt.options[0].value;
            println!("  {} {}\n", "✓".green(), prompt.options[0].label);
            return Ok(value.clone());
        }

        if let Ok(n) = trimmed.parse::<usize>() {
            if n >= 1 && n <= prompt.options.len() {
                let opt = &prompt.options[n - 1];
                println!("  {} {}\n", "✓".green(), opt.label);
                return Ok(opt.value.clone());
            }
        }

        for opt in &prompt.options {
            if trimmed == opt.value.to_ascii_lowercase()
                || trimmed == opt.label.to_ascii_lowercase()
            {
                println!("  {} {}\n", "✓".green(), opt.label);
                return Ok(opt.value.clone());
            }
        }

        println!(
            "  {} Pick a number from 1 to {}.",
            "✗".red(),
            prompt.options.len()
        );
    }
}

fn collect_text(prompt: &Prompt) -> anyhow::Result<String> {
    println!();
    let value = prompt_line(&format!("  {}: ", prompt.prompt))?;
    if value.trim().is_empty() && prompt.required {
        println!("  {} This field is required.", "✗".red());
        return collect_text(prompt);
    }
    println!("  {} {}\n", "✓".green(), prompt.prompt);
    Ok(value.trim().to_string())
}

fn collect_secret(prompt: &Prompt) -> anyhow::Result<String> {
    println!();
    let value = prompt_line(&format!("  {}: ", prompt.prompt))?;
    if value.is_empty() && prompt.required {
        println!("  {} This field is required.", "✗".red());
        return collect_secret(prompt);
    }
    println!("  {} {}\n", "✓".green(), "Set".dimmed());
    Ok(value)
}

fn collect_secret_confirm(prompt: &Prompt) -> anyhow::Result<String> {
    println!();
    let first = prompt_line(&format!("  {}: ", prompt.prompt))?;
    if first.is_empty() && prompt.required {
        println!("  {} This field is required.", "✗".red());
        return collect_secret_confirm(prompt);
    }
    let confirm = prompt_line("  Confirm: ")?;
    if first != confirm {
        println!("  {} Values do not match. Try again.", "✗".red());
        return collect_secret_confirm(prompt);
    }
    println!("  {} {}\n", "✓".green(), "Set".dimmed());
    Ok(first)
}

fn collect_code(prompt: &Prompt) -> anyhow::Result<String> {
    println!();
    let code = prompt_line(&format!("  {}: ", format!("{}:", prompt.prompt).cyan()))?;
    let cleaned = code.trim().replace(' ', "");
    if cleaned.is_empty() {
        println!("  {} Code cannot be empty.", "✗".red());
        return collect_code(prompt);
    }
    Ok(cleaned)
}

fn collect_entropy(prompt: &Prompt) -> anyhow::Result<String> {
    println!();
    println!("  {}", prompt.prompt);
    println!(
        "  {}",
        "Type random characters and press Enter when done:".dimmed()
    );
    let entropy = prompt_line("  > ")?;
    if entropy.trim().is_empty() {
        println!("  {} Using server entropy only.", "→".dimmed());
        return Ok("_server_only".to_string());
    }
    println!(
        "  {} Entropy collected ({} bytes)\n",
        "✓".green(),
        entropy.len()
    );
    Ok(entropy)
}

// ── Text wrapping helper ────────────────────────────────────────────

fn textwrap_simple(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if current.is_empty() {
            current = word.to_string();
        } else if current.len() + 1 + word.len() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}
