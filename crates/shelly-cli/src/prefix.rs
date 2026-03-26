/// Cisco IOS-style prefix command matcher.
///
/// Resolves abbreviated commands like "co l" → "conn list",
/// "v u" → "vault unlock", "d st" → "daemon status".

/// A node in the command tree.
#[derive(Debug)]
pub struct CommandNode {
    pub name: String,
    pub children: Vec<CommandNode>,
    /// If true, this node is a valid complete command.
    pub is_leaf: bool,
}

impl CommandNode {
    fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            children: Vec::new(),
            is_leaf: false,
        }
    }

    fn leaf(name: &str) -> Self {
        Self {
            name: name.into(),
            children: Vec::new(),
            is_leaf: true,
        }
    }

    fn add_child(&mut self, child: CommandNode) -> &mut CommandNode {
        self.children.push(child);
        self.children.last_mut().unwrap()
    }
}

/// Result of prefix resolution.
#[derive(Debug, PartialEq)]
pub enum Resolved {
    /// Matched a single command.
    Match(Vec<String>),
    /// Multiple commands match the prefix.
    Ambiguous(Vec<Vec<String>>),
    /// No command matches.
    NotFound,
    /// Incomplete — node exists but has children and is not a leaf.
    Incomplete(Vec<Vec<String>>),
}

/// Build the full command tree for Shelly CLI.
pub fn build_command_tree() -> CommandNode {
    let mut root = CommandNode::new("root");

    // daemon
    {
        let daemon = root.add_child(CommandNode::new("daemon"));
        daemon.add_child(CommandNode::leaf("start"));
        daemon.add_child(CommandNode::leaf("stop"));
        daemon.add_child(CommandNode::leaf("status"));
    }

    // vault
    {
        let vault = root.add_child(CommandNode::new("vault"));
        vault.add_child(CommandNode::leaf("init"));
        vault.add_child(CommandNode::leaf("unlock"));
        vault.add_child(CommandNode::leaf("lock"));
        vault.add_child(CommandNode::leaf("status"));
        vault.add_child(CommandNode::leaf("change-password"));
    }

    // conn
    {
        let conn = root.add_child(CommandNode::new("conn"));
        conn.add_child(CommandNode::leaf("list"));
        conn.add_child(CommandNode::leaf("show"));
        conn.add_child(CommandNode::leaf("add"));
        conn.add_child(CommandNode::leaf("edit"));
        conn.add_child(CommandNode::leaf("rm"));
        conn.add_child(CommandNode::leaf("import"));
    }

    // group
    {
        let group = root.add_child(CommandNode::new("group"));
        group.add_child(CommandNode::leaf("list"));
        group.add_child(CommandNode::leaf("show"));
        group.add_child(CommandNode::leaf("add"));
        group.add_child(CommandNode::leaf("edit"));
        group.add_child(CommandNode::leaf("rm"));
        group.add_child(CommandNode::leaf("exec"));
        group.add_child(CommandNode::leaf("upload"));
    }

    // tunnel
    {
        let tunnel = root.add_child(CommandNode::new("tunnel"));
        tunnel.add_child(CommandNode::leaf("create"));
        tunnel.add_child(CommandNode::leaf("list"));
        tunnel.add_child(CommandNode::leaf("close"));
    }

    // ssh
    {
        let ssh = root.add_child(CommandNode::new("ssh"));
        ssh.is_leaf = true; // "ssh <name>" is valid
    }

    // exec
    {
        root.add_child(CommandNode::leaf("exec"));
    }

    // scp
    {
        let scp = root.add_child(CommandNode::new("scp"));
        scp.add_child(CommandNode::leaf("upload"));
        scp.add_child(CommandNode::leaf("download"));
    }

    // workflow
    {
        let workflow = root.add_child(CommandNode::new("workflow"));
        workflow.add_child(CommandNode::leaf("list"));
        workflow.add_child(CommandNode::leaf("show"));
        workflow.add_child(CommandNode::leaf("import"));
        workflow.add_child(CommandNode::leaf("rm"));
        workflow.add_child(CommandNode::leaf("run"));
    }

    root
}

/// Resolve a sequence of token prefixes against the command tree.
///
/// Examples:
/// - `["v", "u"]` → `Match(["vault", "unlock"])`
/// - `["co", "l"]` → `Match(["conn", "list"])`
/// - `["d"]` → `Match(["daemon"])` if ambiguous would give multiple, etc.
/// - `["conn"]` → `Incomplete` because conn has subcommands
/// - `["x"]` → `NotFound`
pub fn resolve_prefix(tree: &CommandNode, tokens: &[&str]) -> Resolved {
    if tokens.is_empty() {
        return Resolved::NotFound;
    }

    resolve_recursive(tree, tokens, &mut Vec::new())
}

fn resolve_recursive(
    node: &CommandNode,
    tokens: &[&str],
    path: &mut Vec<String>,
) -> Resolved {
    if tokens.is_empty() {
        // We've consumed all tokens
        if node.is_leaf || node.children.is_empty() {
            return Resolved::Match(path.clone());
        }
        // Node has children — show completions
        let completions: Vec<Vec<String>> = collect_leaves(node, path);
        return Resolved::Incomplete(completions);
    }

    let prefix = tokens[0].to_lowercase();
    let rest = &tokens[1..];

    // Find all children matching this prefix
    let matches: Vec<&CommandNode> = node
        .children
        .iter()
        .filter(|child| child.name.starts_with(&prefix))
        .collect();

    match matches.len() {
        0 => Resolved::NotFound,
        1 => {
            path.push(matches[0].name.clone());
            resolve_recursive(matches[0], rest, path)
        }
        _ => {
            // Check for exact match first
            if let Some(exact) = matches.iter().find(|m| m.name == prefix) {
                path.push(exact.name.clone());
                return resolve_recursive(exact, rest, path);
            }

            // Ambiguous — return all possible completions
            let mut completions = Vec::new();
            for m in &matches {
                let mut p = path.clone();
                p.push(m.name.clone());
                completions.push(p);
            }
            Resolved::Ambiguous(completions)
        }
    }
}

/// Collect all leaf paths under a node.
fn collect_leaves(node: &CommandNode, prefix: &[String]) -> Vec<Vec<String>> {
    let mut results = Vec::new();

    if node.is_leaf {
        results.push(prefix.to_vec());
    }

    for child in &node.children {
        let mut path = prefix.to_vec();
        path.push(child.name.clone());
        if child.is_leaf || child.children.is_empty() {
            results.push(path);
        } else {
            results.extend(collect_leaves(child, &path));
        }
    }

    results
}

/// Compute Levenshtein distance for suggestions.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0; b.len() + 1];

    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b.len()]
}

/// Suggest the closest command for a misspelled prefix.
pub fn suggest_command(tree: &CommandNode, token: &str) -> Option<String> {
    let token_lower = token.to_lowercase();
    tree.children
        .iter()
        .map(|c| (c.name.clone(), levenshtein(&token_lower, &c.name)))
        .filter(|(_, dist)| *dist <= 2)
        .min_by_key(|(_, dist)| *dist)
        .map(|(name, _)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> CommandNode {
        build_command_tree()
    }

    #[test]
    fn test_full_match() {
        let t = tree();
        assert_eq!(
            resolve_prefix(&t, &["vault", "unlock"]),
            Resolved::Match(vec!["vault".into(), "unlock".into()])
        );
    }

    #[test]
    fn test_prefix_match() {
        let t = tree();
        assert_eq!(
            resolve_prefix(&t, &["v", "u"]),
            Resolved::Match(vec!["vault".into(), "unlock".into()])
        );
    }

    #[test]
    fn test_conn_list_prefix() {
        let t = tree();
        assert_eq!(
            resolve_prefix(&t, &["co", "l"]),
            Resolved::Match(vec!["conn".into(), "list".into()])
        );
    }

    #[test]
    fn test_daemon_status() {
        let t = tree();
        assert_eq!(
            resolve_prefix(&t, &["d", "stat"]),
            Resolved::Match(vec!["daemon".into(), "status".into()])
        );
    }

    #[test]
    fn test_daemon_st_ambiguous() {
        let t = tree();
        // "st" matches start, stop, status
        match resolve_prefix(&t, &["d", "st"]) {
            Resolved::Ambiguous(options) => {
                assert_eq!(options.len(), 3);
            }
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn test_not_found() {
        let t = tree();
        assert_eq!(resolve_prefix(&t, &["xyz"]), Resolved::NotFound);
    }

    #[test]
    fn test_ambiguous() {
        let t = tree();
        // "s" matches both "ssh" and "scp"
        let result = resolve_prefix(&t, &["s"]);
        match result {
            Resolved::Ambiguous(options) => {
                assert!(options.len() >= 2);
            }
            _ => panic!("expected Ambiguous, got {result:?}"),
        }
    }

    #[test]
    fn test_incomplete() {
        let t = tree();
        let result = resolve_prefix(&t, &["conn"]);
        match result {
            Resolved::Incomplete(completions) => {
                assert!(!completions.is_empty());
            }
            _ => panic!("expected Incomplete, got {result:?}"),
        }
    }

    #[test]
    fn test_empty_tokens() {
        let t = tree();
        assert_eq!(resolve_prefix(&t, &[]), Resolved::NotFound);
    }

    #[test]
    fn test_exact_match_over_ambiguous() {
        // "ssh" should match exactly even though "scp" also starts with "s"
        let t = tree();
        assert_eq!(
            resolve_prefix(&t, &["ssh"]),
            Resolved::Match(vec!["ssh".into()])
        );
    }

    #[test]
    fn test_group_list_prefix() {
        let t = tree();
        assert_eq!(
            resolve_prefix(&t, &["g", "l"]),
            Resolved::Match(vec!["group".into(), "list".into()])
        );
    }

    #[test]
    fn test_group_show_prefix() {
        let t = tree();
        assert_eq!(
            resolve_prefix(&t, &["gr", "sh"]),
            Resolved::Match(vec!["group".into(), "show".into()])
        );
    }

    #[test]
    fn test_group_add_prefix() {
        let t = tree();
        assert_eq!(
            resolve_prefix(&t, &["g", "a"]),
            Resolved::Match(vec!["group".into(), "add".into()])
        );
    }

    #[test]
    fn test_tunnel_create_prefix() {
        let t = tree();
        assert_eq!(
            resolve_prefix(&t, &["t", "cr"]),
            Resolved::Match(vec!["tunnel".into(), "create".into()])
        );
    }

    #[test]
    fn test_tunnel_c_ambiguous() {
        let t = tree();
        match resolve_prefix(&t, &["t", "c"]) {
            Resolved::Ambiguous(options) => {
                assert_eq!(options.len(), 2); // create, close
            }
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn test_tunnel_list_prefix() {
        let t = tree();
        assert_eq!(
            resolve_prefix(&t, &["t", "l"]),
            Resolved::Match(vec!["tunnel".into(), "list".into()])
        );
    }

    #[test]
    fn test_tunnel_close_prefix() {
        let t = tree();
        assert_eq!(
            resolve_prefix(&t, &["tu", "cl"]),
            Resolved::Match(vec!["tunnel".into(), "close".into()])
        );
    }

    #[test]
    fn test_exec() {
        let t = tree();
        assert_eq!(
            resolve_prefix(&t, &["e"]),
            Resolved::Match(vec!["exec".into()])
        );
    }

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein("vault", "vault"), 0);
        assert_eq!(levenshtein("vaul", "vault"), 1);
        assert_eq!(levenshtein("vaukt", "vault"), 1);
        assert_eq!(levenshtein("xyz", "vault"), 5);
    }

    #[test]
    fn test_suggest_command() {
        let t = tree();
        assert_eq!(suggest_command(&t, "vaul"), Some("vault".into()));
        assert_eq!(suggest_command(&t, "cnn"), Some("conn".into()));
        assert_eq!(suggest_command(&t, "zzz"), None);
    }

    #[test]
    fn test_workflow_list_prefix() {
        let t = tree();
        assert_eq!(
            resolve_prefix(&t, &["w", "l"]),
            Resolved::Match(vec!["workflow".into(), "list".into()])
        );
    }

    #[test]
    fn test_workflow_run_prefix() {
        let t = tree();
        assert_eq!(
            resolve_prefix(&t, &["wo", "ru"]),
            Resolved::Match(vec!["workflow".into(), "run".into()])
        );
    }

    #[test]
    fn test_workflow_import_prefix() {
        let t = tree();
        assert_eq!(
            resolve_prefix(&t, &["w", "i"]),
            Resolved::Match(vec!["workflow".into(), "import".into()])
        );
    }
}
