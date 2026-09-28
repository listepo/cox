#!/bin/sh
# A fake ACP agent for cox-app's external-session tests (T52.4): reads
# newline-delimited JSON-RPC on stdin and answers on stdout, as a real agent
# over stdio does. Every prompt is answered with one message chunk and
# `end_turn`, except the prompt `wait`, which is held until `session/cancel`
# arrives and then answered `cancelled`. It replies `with a key` only when
# the key the entry names reached it.

reply() {
    printf '{"jsonrpc":"2.0","id":"%s","result":%s}\n' "$1" "$2"
}

say() {
    printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"%s"}}}}\n' "$1"
}

held=""
while IFS= read -r line; do
    id=$(printf '%s\n' "$line" | grep -o '"id":"[^"]*"' | head -n 1 | cut -d '"' -f 4)
    method=$(printf '%s\n' "$line" | grep -o '"method":"[^"]*"' | head -n 1 | cut -d '"' -f 4)
    case "$method" in
    initialize)
        reply "$id" '{"protocolVersion":1,"agentCapabilities":{}}'
        ;;
    session/new)
        reply "$id" '{"sessionId":"fake-1"}'
        ;;
    session/prompt)
        case "$line" in
        *'"text":"wait"'*)
            held="$id"
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
    esac
done
