mod client;
mod commands;
mod prefix;

use prefix::{Resolved, build_command_tree, resolve_prefix, suggest_command};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        print_help();
        return;
    }

    if args[0] == "--version" || args[0] == "-V" {
        println!("rivet {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    let tree = build_command_tree();

    // Split args into command tokens and extra arguments.
    // Command tokens are prefix-matched; extra args are passed through.
    // We try progressively more tokens until resolution fails.
    let (resolved_cmd, extra_start) = resolve_command(&tree, &args);

    match resolved_cmd {
        Resolved::Match(cmd) => {
            let extra_args: Vec<String> = args[extra_start..].to_vec();
            if let Err(e) = commands::dispatch(&cmd, &extra_args).await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Resolved::Ambiguous(options) => {
            eprintln!("Ambiguous command. Did you mean one of:");
            for opt in &options {
                eprintln!("  rivet {}", opt.join(" "));
            }
            std::process::exit(2);
        }
        Resolved::Incomplete(completions) => {
            eprintln!("Incomplete command. Available subcommands:");
            for comp in &completions {
                eprintln!("  rivet {}", comp.join(" "));
            }
            std::process::exit(2);
        }
        Resolved::NotFound => {
            let first = &args[0];
            if let Some(suggestion) = suggest_command(&tree, first) {
                eprintln!("Unknown command: '{first}'. Did you mean '{suggestion}'?");
            } else {
                eprintln!("Unknown command: '{first}'. Run 'rivet --help' for usage.");
            }
            std::process::exit(2);
        }
    }
}

/// Try to resolve as many tokens as possible as command parts.
fn resolve_command(tree: &prefix::CommandNode, args: &[String]) -> (Resolved, usize) {
    let mut best_match = Resolved::NotFound;
    let mut best_consumed = 0;

    // Try consuming 1, 2, ... tokens as command parts
    for n in 1..=args.len().min(3) {
        let tokens: Vec<&str> = args[..n].iter().map(|s| s.as_str()).collect();
        let result = resolve_prefix(tree, &tokens);

        match &result {
            Resolved::Match(_) => {
                best_match = result;
                best_consumed = n;
                // Don't break — try consuming more tokens for deeper match
            }
            Resolved::Ambiguous(_) | Resolved::Incomplete(_) => {
                // Only use ambiguous/incomplete if we don't have a better match
                if matches!(best_match, Resolved::NotFound) {
                    best_match = result;
                    best_consumed = n;
                }
            }
            Resolved::NotFound => {
                // If we already have a match and adding more tokens fails, stop
                if !matches!(best_match, Resolved::NotFound) {
                    break;
                }
            }
        }
    }

    (best_match, best_consumed)
}

fn print_help() {
    println!(
        "rivet {} — SSH connection manager",
        env!("CARGO_PKG_VERSION")
    );
    println!();
    println!("USAGE:");
    println!("  rivet <command> [args...]");
    println!("  Commands support Cisco IOS-style prefix matching (e.g., 'co l' = 'conn list')");
    println!();
    println!("COMMANDS:");
    println!("  daemon start|stop|status    Manage the rivetd daemon");
    println!("  vault  init|unlock|lock|status|change-password");
    println!("                              Manage the encrypted vault");
    println!("  conn   list|show|add|edit|rm|import");
    println!("                              Manage SSH connections");
    println!("  ssh    <name> [args...]      Open interactive SSH session");
    println!("  exec   <name> <command>      Execute command on remote host");
    println!("  scp    upload|download       Transfer files");
    println!();
    println!("EXAMPLES:");
    println!("  rivet daemon start          Start the daemon");
    println!("  rivet v u                   Unlock vault (prefix match)");
    println!("  rivet co l                  List connections (prefix match)");
    println!("  rivet ssh prod-web          SSH to 'prod-web'");
    println!("  rivet exec prod-web uptime  Run 'uptime' on prod-web");
    println!("  rivet scp up prod-web ./f.txt /tmp/f.txt");
    println!("                              Upload file to prod-web");
}
