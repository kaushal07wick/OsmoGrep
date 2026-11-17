//! Natural-language routing hints; transport identity checks stay in the team store.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Intent {
    None,
    Allow(bool),
    Deny,
}
fn peer_word(word: &str) -> bool {
    matches!(
        word,
        "team"
            | "agent"
            | "agents"
            | "peer"
            | "peers"
            | "instance"
            | "instances"
            | "session"
            | "sessions"
            | "teammate"
            | "teammates"
            | "collaborator"
            | "collaborators"
    )
}
pub(super) fn communication_intent(prompt: &str, contextual: bool) -> Intent {
    let previous_contact = contextual;
    let lower = prompt.to_lowercase().replace('’', "'");
    let mut quoted = false;
    let mut quote = ' ';
    let mut text = String::new();
    for c in lower.chars() {
        if quoted {
            if c == quote {
                quoted = false;
            }
            text.push(' ');
        } else if matches!(c, '"' | '`') {
            quoted = true;
            quote = c;
            text.push(' ');
        } else {
            text.push(c);
        }
    }
    let text = text
        .replace(" and ", ";")
        .replace(" then ", ";")
        .replace(" but ", ";");
    // A named agent earlier in this request can be the referent of "message him".
    let contextual = contextual || text.split(|c: char| !c.is_alphanumeric()).any(peer_word);
    let mut intent = Intent::None;
    for clause in text.split(['.', '!', '?', ';', ',', '\n']) {
        let words = clause
            .split_whitespace()
            .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-'))
            .collect::<Vec<_>>();
        let Some(verb) = words.iter().position(|s| {
            matches!(
                *s,
                "talk"
                    | "speak"
                    | "message"
                    | "contact"
                    | "ping"
                    | "tell"
                    | "ask"
                    | "coordinate"
                    | "sync"
                    | "communicate"
                    | "collaborate"
                    | "send"
                    | "reply"
                    | "discuss"
                    | "chat"
                    | "reach"
                    | "get"
                    | "check"
                    | "let"
                    | "broadcast"
            )
        }) else {
            continue;
        };
        let before = &words[..verb];
        let after = &words[verb + 1..];
        let peer = after
            .iter()
            .take(14)
            .position(|s| (contextual && matches!(*s, "him" | "her" | "them")) || peer_word(s))
            .or_else(|| {
                (previous_contact
                    && words[verb] == "reply"
                    && !after.iter().any(|s| matches!(*s, "me" | "us")))
                .then_some(0)
            });
        let negative = before.iter().any(|s| {
            matches!(
                *s,
                "not"
                    | "don't"
                    | "dont"
                    | "never"
                    | "no"
                    | "shouldn't"
                    | "shouldnt"
                    | "can't"
                    | "cannot"
            )
        });
        let human = after
            .iter()
            .take(peer.unwrap_or(after.len()).min(14))
            .any(|s| matches!(*s, "me" | "us"));
        if negative
            && !human
            && (peer.is_some() || matches!(words[verb], "message" | "contact" | "ping" | "send"))
        {
            intent = Intent::Deny;
            continue;
        }
        let Some(peer) = peer else { continue };
        if human
            || negative
            || before.iter().any(|s| {
                matches!(
                    *s,
                    "how"
                        | "whether"
                        | "explain"
                        | "example"
                        | "implement"
                        | "design"
                        | "document"
                        | "source"
                        | "code"
                )
            })
        {
            continue;
        }
        if verb > 0
            && !before.last().is_some_and(|s| {
                matches!(
                    *s,
                    "you"
                        | "u"
                        | "please"
                        | "to"
                        | "also"
                        | "now"
                        | "first"
                        | "then"
                        | "and"
                        | "yes"
                        | "yeah"
                        | "yep"
                        | "ok"
                        | "okay"
                )
            })
        {
            continue;
        }
        let target = &after[..peer];
        if target.iter().any(|s| matches!(*s, "about" | "how")) {
            continue;
        }
        if matches!(
            words[verb],
            "talk" | "speak" | "communicate" | "send" | "chat" | "reach"
        ) && !target.iter().any(|s| matches!(*s, "to" | "with"))
        {
            continue;
        }
        if words[verb] == "reach" && !target.contains(&"out") {
            continue;
        }
        if words[verb] == "get"
            && !(target.contains(&"in") && target.contains(&"touch") && target.contains(&"with"))
        {
            continue;
        }
        if words[verb] == "check" && !target.contains(&"with") {
            continue;
        }
        if words[verb] == "let" && !after[peer + 1..].contains(&"know") {
            continue;
        }
        intent = Intent::Allow(
            words[verb] == "broadcast" || target.iter().any(|s| matches!(*s, "all" | "every")),
        );
    }
    if intent == Intent::None && previous_contact {
        let words = text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        let selecting_sender = words.first() == Some(&"the")
            && words.iter().any(|s| peer_word(s))
            && words.iter().any(|s| matches!(*s, "which" | "who" | "that"))
            && words
                .iter()
                .any(|s| matches!(*s, "sent" | "messaged" | "contacted"))
            && !words
                .iter()
                .any(|s| matches!(*s, "not" | "never" | "dont" | "no"));
        if selecting_sender {
            return Intent::Allow(false);
        }
    }
    intent
}
pub fn communication_requested(prompt: &str) -> bool {
    matches!(communication_intent(prompt, false), Intent::Allow(_))
}
pub fn mentions_peer(prompt: &str) -> bool {
    let lower = prompt.to_ascii_lowercase();
    communication_requested(prompt)
        || [
            "team",
            "collaborat",
            "other agent",
            "another agent",
            "other instance",
            "another instance",
            "other session",
            "another session",
            "agent working",
            "agent's status",
            "agent id",
            "session id",
            "list agents",
            "list the agents",
            "active agents",
            "running agents",
            "who else is working",
        ]
        .iter()
        .any(|s| lower.contains(s))
}
