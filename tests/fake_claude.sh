#!/usr/bin/env bash
# A fake `claude` CLI for tests and the smoke run. Emits canned output so the whole
# book-generation and Connect Claude paths can be exercised with zero real usage and no
# auth. Behavior is selected by the subcommand and the FAKE_CLAUDE_MODE env var:
#   (unset)      success: stream-json init + deltas + assistant + result with a chapter
#   rate_limit   api_retry rate_limit, then error_during_execution
#   logged_out   api_retry authentication_failed, then error_during_execution
#   max_turns    result subtype error_max_turns
#   hang         sleep forever (the app-side timeout must kill us)
#   badcode      setup-token only: reject the pasted code
set -u

mode="${FAKE_CLAUDE_MODE:-success}"

# ---- claude auth status --json ----
if [[ "${1:-}" == "auth" && "${2:-}" == "status" ]]; then
    if [[ "$mode" == "logged_out" ]]; then
        echo '{"loggedIn":false}'
    else
        echo '{"loggedIn":true,"authMethod":"claude.ai","subscriptionType":"max"}'
    fi
    exit 0
fi

# ---- claude setup-token (PTY-driven Connect Claude flow) ----
if [[ "${1:-}" == "setup-token" ]]; then
    # Some ANSI noise and a spinner, like the real thing.
    printf '\033[1mWelcome to Fake Claude v0.0.1\033[0m\n'
    printf 'Opening browser to sign in\xe2\x80\xa6\n'
    sleep 0.1
    printf "Browser didn't open? Use the url below to sign in (c to copy)\n\n"
    printf 'https://claude.example.test/cai/oauth/authorize?code=true&client_id=fake&state=fakestate\n\n'
    sleep 0.1
    printf 'Paste code here if prompted > '
    IFS= read -r code || code=""
    if [[ "$mode" == "badcode" || -z "$code" ]]; then
        printf '\nInvalid code. Please try again.\n'
        exit 1
    fi
    printf '\nSuccess! Your long-lived token:\n'
    printf 'sk-ant-oat01-FAKETOKEN1234567890abcdefghij\n'
    printf 'Store it somewhere safe.\n'
    exit 0
fi

# ---- claude -p ... (headless generation) ----
# Consume the prompt from stdin like the real CLI; optionally dump it for tests that
# assert what actually reached the agent.
if [[ -n "${FAKE_CLAUDE_DUMP_PROMPT:-}" ]]; then
    cat > "$FAKE_CLAUDE_DUMP_PROMPT" || true
else
    cat >/dev/null || true
fi

if [[ "$mode" == "hang" ]]; then
    # exec so the kill from the app-side watchdog hits the sleeping process itself
    # (otherwise an orphaned sleep would keep the stdout pipe open).
    exec sleep 600
fi

init='{"type":"system","subtype":"init","cwd":".","session_id":"fake-session-0001","model":"claude-fake-1","tools":[],"mcp_servers":[],"plugins":[{"name":"novelist","path":"/fake"}],"slash_commands":["novelist:write-chapter"],"apiKeySource":"none"}'

case "$mode" in
    rate_limit)
        echo "$init"
        echo '{"type":"api_retry","error":"rate_limit","attempt":1,"max_retries":1,"retry_delay_ms":10}'
        echo '{"type":"result","subtype":"error_during_execution","is_error":true,"session_id":"fake-session-0001"}'
        exit 1
        ;;
    logged_out)
        echo "$init"
        echo '{"type":"api_retry","error":"authentication_failed","attempt":1,"max_retries":1,"retry_delay_ms":10}'
        echo '{"type":"result","subtype":"error_during_execution","is_error":true,"session_id":"fake-session-0001"}'
        exit 1
        ;;
    max_turns)
        echo "$init"
        echo '{"type":"result","subtype":"error_max_turns","is_error":true,"session_id":"fake-session-0001"}'
        exit 1
        ;;
    *)
        echo "$init"
        echo '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"===TITLE===\nThe Fake Chapter\n===CHAPTER===\nRain hit the smoke-test "}}}'
        echo '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"window while the parser waited, counting quiet seconds."}}}'
        chapter='===TITLE===\nThe Fake Chapter\n===CHAPTER===\nRain hit the smoke-test window while the parser waited, counting quiet seconds. Nobody came in. The fake author kept writing anyway, one deliberate sentence after another, until the page held enough words to type. A kettle clicked off in the next room. Somewhere below, a door closed twice, which meant the landlady knew.\n===BIBLE===\nCAST: The Parser - patient; wants a clean chapter.\nFACTS/WORLD: It rains during smoke tests.\nTHREADS: Who closed the door twice?\nVOICE: third limited, past tense, dry.\nTIMELINE: One evening.\nPLANTED: The landlady signal.\n===END==='
        printf '{"type":"assistant","message":{"content":[{"type":"text","text":"%s"}]}}\n' "$chapter"
        printf '{"type":"result","subtype":"success","is_error":false,"result":"%s","session_id":"fake-session-0001","num_turns":1,"total_cost_usd":0}\n' "$chapter"
        exit 0
        ;;
esac
