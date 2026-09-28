#!/bin/sh
# A fake ACP agent for cox-app's external-session tests (T52.4-T52.6): reads
# newline-delimited JSON-RPC on stdin and answers on stdout, as a real agent
# over stdio does. Every prompt is answered with one message chunk and
# `end_turn`, except two. The prompt `wait` is held until `session/cancel`
# arrives and then answered `cancelled`. The prompt `ask` sends
# `session/request_permission` for `make deploy`, holds the prompt until the
# client answers, says which option came back (`answered once`), and then
# ends the turn. It replies `with a key` only when the key the entry names
# reached it. Started with the argument `load`, it advertises `loadSession`
# and reopens its one session, `fake-1`, replaying a line cox must not show
# twice; without it, it cannot reopen anything.

caps='{}'
if [ "$1" = "load" ]; then
    caps='{"loadSession":true}'
fi

fail() {
    printf '{"jsonrpc":"2.0","id":"%s","error":{"code":-32602,"message":"%s"}}\n' "$1" "$2"
}

reply() {
    printf '{"jsonrpc":"2.0","id":"%s","result":%s}\n' "$1" "$2"
}

say() {
    printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"%s"}}}}\n' "$1"
}

ask() {
    printf '%s\n' '{"jsonrpc":"2.0","id":"perm-1","method":"session/request_permission","params":{"sessionId":"fake-1","toolCall":{"toolCallId":"call-1","title":"make deploy","kind":"execute","rawInput":{"command":"make deploy"}},"options":[{"optionId":"once","name":"Allow once","kind":"allow_once"},{"optionId":"always","name":"Always allow","kind":"allow_always"},{"optionId":"no","name":"Reject","kind":"reject_once"}]}}'
}

held=""
asking=""
while IFS= read -r line; do
    id=$(printf '%s\n' "$line" | grep -o '"id":"[^"]*"' | head -n 1 | cut -d '"' -f 4)
    method=$(printf '%s\n' "$line" | grep -o '"method":"[^"]*"' | head -n 1 | cut -d '"' -f 4)
    case "$method" in
    initialize)
        reply "$id" "{\"protocolVersion\":1,\"agentCapabilities\":$caps}"
        ;;
    session/new)
        reply "$id" '{"sessionId":"fake-1"}'
        ;;
    session/load)
        case "$line" in
        *'"sessionId":"fake-1"'*)
            say "replayed by session/load"
            reply "$id" 'null'
            ;;
        *)
            fail "$id" "no such session"
            ;;
        esac
        ;;
    session/prompt)
        case "$line" in
        *'"text":"wait"'*)
            held="$id"
            ;;
        *'"text":"ask"'*)
            asking="$id"
            ask
            ;;
        *)
            if [ -n "$COX_FAKE_ACP_KEY" ]; then
                say "hello from the fake agent with a key"
            else
                say "hello from the fake agent"
            fi
            reply "$id" '{"stopReason":"end_turn"}'
            ;;
        esac
        ;;
    session/cancel)
        if [ -n "$held" ]; then
            reply "$held" '{"stopReason":"cancelled"}'
            held=""
        fi
        ;;
    "")
        # The client's response to `perm-1`: the option it picked, or
        # `cancelled` when it picked none.
        if [ "$id" = "perm-1" ] && [ -n "$asking" ]; then
            option=$(printf '%s\n' "$line" | grep -o '"optionId":"[^"]*"' | head -n 1 | cut -d '"' -f 4)
            say "answered ${option:-cancelled}"
            reply "$asking" '{"stopReason":"end_turn"}'
            asking=""
        fi
        ;;
    esac
done
