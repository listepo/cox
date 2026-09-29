# English: the source and fallback locale (docs/i18n.md). Every message id
# lives here first; ru and uk may omit one, and it then resolves from here.

## Terms

-brand-name = Cox

## Messages

app-title = { -brand-name }
settings-title = Settings
quit-app = Quit { -brand-name }
welcome-user = Welcome to { -brand-name }, { $name }!

# $count (number): open sessions in the sidebar.
session-count = { $count ->
    [one] { $count } session
   *[other] { $count } sessions
}

# $name (string): who made the change; $count (number): changed files.
files-changed = { $name } changed { $count ->
    [one] { $count } file
   *[other] { $count } files
}

# Deliberately absent from ru and uk: exercises the per-message fallback.
send-feedback = Send feedback
