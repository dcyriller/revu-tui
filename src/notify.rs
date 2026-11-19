//! The macOS notification for an MR that lands in To review, and the way back to revu when it is
//! clicked. `terminal-notifier` runs a line on the click that switches tmux to revu's session, window and
//! pane and brings its terminal in front; without it, `osascript` shows a notification nothing happens on.
//! Every value goes to a program as an argument, never into a script, so an MR title cannot run
//! AppleScript or shell.

/// `display notification`, with the title and the body as arguments.
const OSASCRIPT: [&str; 7] =
    ["osascript", "-e", "on run argv", "-e", "display notification (item 2 of argv) with title (item 1 of argv)", "-e", "end run"];
const TERMINAL_NOTIFIER: &str = "terminal-notifier";
/// The group of every notification revu shows, so a new one replaces the last instead of stacking.
const GROUP: &str = "revu";
/// The app that started the shell revu runs in; macOS sets it for every program an app starts.
const BUNDLE_ID: &str = "__CFBundleIdentifier";

/// What shows a notification on this machine, read once at start.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Notifier {
    /// `terminal-notifier` as revu's `PATH` finds it; without it, `osascript`.
    program: Option<String>,
    /// The bundle id of the terminal revu runs in, which a click brings in front.
    terminal: Option<String>,
    /// The line a click runs: `tmux` switching to revu's pane, when revu runs in tmux.
    back: Option<String>,
}

impl Notifier {
    /// What the machine says: `__CFBundleIdentifier` names the terminal, `TMUX` the server and `TMUX_PANE` revu's pane.
    pub(crate) fn detect(env: impl Fn(&str) -> Option<String>, locate: impl Fn(&str) -> Option<String>) -> Self {
        Self { program: locate(TERMINAL_NOTIFIER), terminal: env(BUNDLE_ID).filter(|id| !id.is_empty()), back: tmux_back(&env, &locate) }
    }

    /// The program and its words that show `title` over `body`.
    pub(crate) fn words(&self, title: &str, body: &str) -> Vec<String> {
        let Some(program) = &self.program else {
            return OSASCRIPT.iter().map(|word| (*word).to_owned()).chain([title.to_owned(), body.to_owned()]).collect();
        };
        let mut words = [program.as_str(), "-title", title, "-message", body, "-group", GROUP].map(str::to_owned).to_vec();
        if let Some(terminal) = &self.terminal {
            words.extend(["-activate".to_owned(), terminal.clone()]);
        }
        if let Some(back) = &self.back {
            words.extend(["-execute".to_owned(), back.clone()]);
        }
        words
    }
}

/// `tmux -S /private/tmp/tmux-501/default switch-client -t '%5'`: a pane as the target switches the client
/// to its session, window and pane, so the click works from any session. The server comes from `TMUX`,
/// and tmux is a full path because the click runs with the notifier's own `PATH`, which does not know Homebrew.
fn tmux_back(env: &impl Fn(&str) -> Option<String>, locate: &impl Fn(&str) -> Option<String>) -> Option<String> {
    let pane = env("TMUX_PANE").filter(|pane| !pane.is_empty())?;
    let socket = crate::agent::tmux_socket(env)?;
    let tmux = locate("tmux").unwrap_or_else(|| "tmux".to_owned());
    let words = [tmux.as_str(), "-S", &socket, "switch-client", "-t", &pane];
    Some(shell_words::join(words))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TITLE: &str = "revu · to review";
    const BODY: &str = "!42 fix(checkout): it's \"quoted\" · nina";

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| pairs.iter().find(|(key, _)| *key == name).map(|(_, value)| (*value).to_owned())
    }

    fn under(folder: &str) -> impl Fn(&str) -> Option<String> {
        let folder = folder.to_owned();
        move |program| Some(format!("{folder}/{program}"))
    }

    #[test]
    fn in_tmux_the_click_switches_to_revus_pane_and_brings_the_terminal_in_front() {
        let machine =
            [("__CFBundleIdentifier", "com.mitchellh.ghostty"), ("TMUX", "/private/tmp/tmux-501/default,1234,2"), ("TMUX_PANE", "%5")];
        let words = Notifier::detect(env(&machine), under("/opt/homebrew/bin")).words(TITLE, BODY);
        let back = "/opt/homebrew/bin/tmux -S /private/tmp/tmux-501/default switch-client -t '%5'";
        let wanted = [
            "/opt/homebrew/bin/terminal-notifier",
            "-title",
            TITLE,
            "-message",
            BODY,
            "-group",
            "revu",
            "-activate",
            "com.mitchellh.ghostty",
            "-execute",
            back,
        ];
        assert_eq!(words, wanted);
    }

    #[test]
    fn outside_tmux_the_click_only_brings_the_terminal_in_front() {
        let words =
            Notifier::detect(env(&[("__CFBundleIdentifier", "com.googlecode.iterm2")]), under("/opt/homebrew/bin")).words(TITLE, BODY);
        let wanted = [
            "/opt/homebrew/bin/terminal-notifier",
            "-title",
            TITLE,
            "-message",
            BODY,
            "-group",
            "revu",
            "-activate",
            "com.googlecode.iterm2",
        ];
        assert_eq!(words, wanted);
        let words = Notifier::detect(env(&[("TMUX_PANE", "%5")]), under("/opt/homebrew/bin")).words(TITLE, BODY);
        let wanted = ["/opt/homebrew/bin/terminal-notifier", "-title", TITLE, "-message", BODY, "-group", "revu"];
        assert_eq!(words, wanted, "a pane with no server is no tmux");
    }

    #[test]
    fn a_tmux_revus_path_does_not_find_is_called_by_name() {
        let machine = [("TMUX", "/tmp/tmux-501/default,1,0"), ("TMUX_PANE", "%82")];
        let locate = |program: &str| (program == TERMINAL_NOTIFIER).then(|| "/usr/local/bin/terminal-notifier".to_owned());
        let words = Notifier::detect(env(&machine), locate).words(TITLE, BODY);
        assert_eq!(words.last().unwrap(), "tmux -S /tmp/tmux-501/default switch-client -t '%82'");
    }

    #[test]
    fn without_terminal_notifier_osascript_shows_the_text_as_arguments() {
        let machine = [("__CFBundleIdentifier", "com.mitchellh.ghostty"), ("TMUX", "/tmp/tmux-501/default,1,0"), ("TMUX_PANE", "%5")];
        let words = Notifier::detect(env(&machine), |_| None).words(TITLE, BODY);
        let script = "display notification (item 2 of argv) with title (item 1 of argv)";
        assert_eq!(words, ["osascript", "-e", "on run argv", "-e", script, "-e", "end run", TITLE, BODY]);
    }
}
