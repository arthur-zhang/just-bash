use clap::Parser;
use std::io::Read;
use just_bash::bash::{Bash, BashOptions};

#[derive(Parser)]
#[command(name = "just-bash")]
#[command(about = "A secure bash environment for AI agents")]
#[command(version)]
struct Cli {
    /// Execute the script from command line argument
    #[arg(short = 'c')]
    script: Option<String>,

    /// Exit immediately if a command exits with non-zero status
    #[arg(short = 'e', long = "errexit")]
    errexit: bool,

    /// Working directory within the sandbox
    #[arg(long = "cwd")]
    cwd: Option<String>,

    /// Output results as JSON (stdout, stderr, exitCode)
    #[arg(long = "json")]
    json: bool,

    /// Script file to execute
    #[arg()]
    script_file: Option<String>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Determine script source: -c, file, or stdin
    let script = if let Some(s) = cli.script {
        s
    } else if let Some(ref file) = cli.script_file {
        match std::fs::read_to_string(file) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("Error: Cannot read script file: {}: {}", file, e);
                std::process::exit(1);
            }
        }
    } else {
        // Use std::io::IsTerminal (stable since Rust 1.70) for TTY detection
        use std::io::IsTerminal;
        if std::io::stdin().is_terminal() {
            eprintln!("Error: No script provided. Use -c 'script', provide a script file, or pipe via stdin.");
            std::process::exit(1);
        }
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf).unwrap_or_default();
        buf
    };

    if script.trim().is_empty() {
        if cli.json {
            println!("{}", serde_json::json!({"stdout": "", "stderr": "", "exitCode": 0}));
        }
        std::process::exit(0);
    }

    let mut bash = Bash::new(BashOptions {
        cwd: cli.cwd,
        ..Default::default()
    }).await;

    // Prepend set -e if errexit
    let final_script = if cli.errexit {
        format!("set -e\n{}", script)
    } else {
        script
    };

    let result = bash.exec(&final_script, None).await;

    if cli.json {
        println!("{}", serde_json::json!({
            "stdout": result.stdout,
            "stderr": result.stderr,
            "exitCode": result.exit_code,
        }));
    } else {
        if !result.stdout.is_empty() {
            print!("{}", result.stdout);
        }
        if !result.stderr.is_empty() {
            eprint!("{}", result.stderr);
        }
    }

    std::process::exit(result.exit_code);
}
