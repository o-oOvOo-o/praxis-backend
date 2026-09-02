use tree_sitter::Node;
use tree_sitter::Tree;

const PLAIN_NODES: &[&str] = &[
    "program",
    "list",
    "pipeline",
    "command",
    "command_name",
    "word",
    "string",
    "string_content",
    "raw_string",
    "number",
    "concatenation",
];
const PLAIN_TOKENS: &[&str] = &["&&", "||", ";", "|", "\"", "'"];

pub(super) struct Syntax<'source> {
    source: &'source str,
}

impl<'source> Syntax<'source> {
    pub(super) fn new(source: &'source str) -> Self {
        Self { source }
    }

    pub(super) fn plain_sequence(&self, tree: &Tree) -> Option<Vec<Vec<String>>> {
        let root = tree.root_node();
        if root.has_error() {
            return None;
        }
        let mut command_nodes = self.audit_plain_tree(root)?;
        command_nodes.sort_unstable_by_key(Node::start_byte);
        command_nodes
            .into_iter()
            .map(|node| self.decode_plain_command(node))
            .collect()
    }

    pub(super) fn single_heredoc_prefix(&self, tree: &Tree) -> Option<Vec<String>> {
        let root = tree.root_node();
        if root.has_error() {
            return None;
        }
        let mut stack = vec![root];
        let mut command = None;
        let mut saw_heredoc = false;
        while let Some(node) = stack.pop() {
            match node.kind() {
                "heredoc_redirect" => saw_heredoc = true,
                "command" if command.replace(node).is_some() => return None,
                _ => {}
            }
            let mut cursor = node.walk();
            stack.extend(node.named_children(&mut cursor));
        }
        saw_heredoc.then_some(())?;
        self.decode_heredoc_command(command?)
    }

    fn audit_plain_tree<'tree>(&self, root: Node<'tree>) -> Option<Vec<Node<'tree>>> {
        let mut stack = vec![root];
        let mut commands = Vec::new();
        while let Some(node) = stack.pop() {
            let kind = node.kind();
            if node.is_named() {
                PLAIN_NODES.contains(&kind).then_some(())?;
                if kind == "command" {
                    commands.push(node);
                }
            } else {
                (PLAIN_TOKENS.contains(&kind) || kind.trim().is_empty()).then_some(())?;
            }
            let mut cursor = node.walk();
            stack.extend(node.children(&mut cursor));
        }
        Some(commands)
    }

    fn decode_plain_command(&self, command: Node<'_>) -> Option<Vec<String>> {
        (command.kind() == "command").then_some(())?;
        let mut argv = Vec::new();
        let mut cursor = command.walk();
        for argument in command.named_children(&mut cursor) {
            match argument.kind() {
                "command_name" => argv.push(self.command_name(argument)?),
                "word" | "number" => argv.push(self.text(argument)?.to_owned()),
                "string" => argv.push(self.quoted(argument, '"', "string_content")?),
                "raw_string" => argv.push(self.quoted(argument, '\'', "")?),
                "concatenation" => argv.push(self.concatenation(argument)?),
                _ => return None,
            }
        }
        Some(argv)
    }

    fn command_name(&self, node: Node<'_>) -> Option<String> {
        let child = node.named_child(0)?;
        (child.kind() == "word").then_some(())?;
        self.text(child).map(str::to_owned)
    }

    fn concatenation(&self, node: Node<'_>) -> Option<String> {
        let mut value = String::new();
        let mut cursor = node.walk();
        for part in node.named_children(&mut cursor) {
            match part.kind() {
                "word" | "number" => value.push_str(self.text(part)?),
                "string" => value.push_str(&self.quoted(part, '"', "string_content")?),
                "raw_string" => value.push_str(&self.quoted(part, '\'', "")?),
                _ => return None,
            }
        }
        (!value.is_empty()).then_some(value)
    }

    fn quoted(&self, node: Node<'_>, delimiter: char, allowed_child: &str) -> Option<String> {
        let mut cursor = node.walk();
        if node
            .named_children(&mut cursor)
            .any(|child| child.kind() != allowed_child)
        {
            return None;
        }
        self.text(node)?
            .strip_prefix(delimiter)?
            .strip_suffix(delimiter)
            .map(str::to_owned)
    }

    fn decode_heredoc_command(&self, command: Node<'_>) -> Option<Vec<String>> {
        let mut argv = Vec::new();
        let mut cursor = command.walk();
        for child in command.named_children(&mut cursor) {
            match child.kind() {
                "command_name" => {
                    let word = child.named_child(0)?;
                    self.literal(word).then_some(())?;
                    argv.push(self.text(word)?.to_owned());
                }
                "word" | "number" => {
                    self.literal(child).then_some(())?;
                    argv.push(self.text(child)?.to_owned());
                }
                "variable_assignment" | "comment" => {}
                "heredoc_body"
                | "simple_heredoc_body"
                | "heredoc_redirect"
                | "herestring_redirect"
                | "file_redirect"
                | "redirected_statement" => {}
                _ => return None,
            }
        }
        (!argv.is_empty()).then_some(argv)
    }

    fn literal(&self, node: Node<'_>) -> bool {
        if !matches!(node.kind(), "word" | "number") {
            return false;
        }
        let mut cursor = node.walk();
        node.named_children(&mut cursor).next().is_none()
    }

    fn text(&self, node: Node<'_>) -> Option<&'source str> {
        node.utf8_text(self.source.as_bytes()).ok()
    }
}
