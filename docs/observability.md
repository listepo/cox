# Observability

cox emits `tracing` spans and events for every session, turn, provider
round and tool execution. They always land in a local JSON log; with
`telemetry.otel = true` they are also exported as OpenTelemetry traces and
logs over OTLP/HTTP to any compatible backend (SigNoz, Jaeger, Grafana
Tempo, Honeycomb, hosted collectors). There is no vendor SDK in the binary:
the standard `OTEL_*` environment variables are the whole portability
contract.

## Local logs (always on)

Every run appends JSON lines to `~/.cox/logs/cox.log.<date>` (under
`COX_HOME` when set), one line per event with the enclosing span ids, so a
failed run can be read without any backend. `core.log_level` is a `tracing`
filter string (`info`, `debug`, `cox_core=trace,info`).

## Turning on OTLP export

```toml
# ~/.cox/config.toml
[telemetry]
otel = true
# optional: where to send; empty means the OTEL_* variables decide
endpoint = "http://localhost:4318"
```

or, for one run: `cox --set telemetry.otel=true -p "..."`.

Standard variables the exporter honours (the `opentelemetry-otlp` defaults):

| Variable | Meaning | Default |
| --- | --- | --- |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | base URL; cox appends `/v1/traces` and `/v1/logs` | `http://localhost:4318` |
| `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT`, `OTEL_EXPORTER_OTLP_LOGS_ENDPOINT` | per-signal full URLs | derived from the base |
| `OTEL_EXPORTER_OTLP_HEADERS` | `key=value,key2=value2`, e.g. an API key for a hosted backend | none |
| `OTEL_EXPORTER_OTLP_TIMEOUT` | export timeout in ms | `10000` |
| `OTEL_SERVICE_NAME` | resource `service.name` | `cox` |
| `OTEL_RESOURCE_ATTRIBUTES` | extra resource attributes, `k=v,k2=v2` (`deployment.environment=dev,host.name=$HOST`) | none |
| `OTEL_INSTRUMENTATION_GENAI_CAPTURE_MESSAGE_CONTENT` | `true` records prompts, completions, tool arguments and tool output on spans | unset (off) |

`telemetry.endpoint` is a convenience for the first variable and overrides
it when both are set. `cox` needs the `otel` cargo feature (on by default);
a build without it fails at startup when `telemetry.otel` is true rather
than silently dropping data.

### Content capture is opt-in

By default spans carry operational metadata only: ids, models, token
counts, cost, latency, stop reasons, tool names, subjects, risk, byte
counts and outcomes. Prompts, completions, tool arguments and tool results
are recorded only when `OTEL_INSTRUMENTATION_GENAI_CAPTURE_MESSAGE_CONTENT`
is `true`, because they contain your source code and, sometimes, secrets
that a tool printed. Turn it on for a local stack, never for a shared
backend you do not control.

## What cox emits

### Spans

One session is one trace. Every turn hangs off the session span, and every
provider round and tool call hangs off its turn, so opening a trace shows the
whole agent loop in order.

| Span | Emitted for | Key attributes |
| --- | --- | --- |
| `invoke_agent cox` | the session | `gen_ai.operation.name`, `gen_ai.agent.name`, `gen_ai.conversation.id`, `cox.session.id`, `cox.session.parent_id` (subagents), `cox.cwd` |
| `invoke_agent cox.turn` | one user turn | `cox.turn.id`, `cox.turn`, `cox.job`, `cox.tier`, `cox.turn.stop_reason` |
| `chat` | one provider round | `gen_ai.provider.name`, `gen_ai.request.model`, `gen_ai.response.model`, `gen_ai.response.finish_reasons`, `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens`, `cox.usage.cache_read_tokens`, `cox.usage.cache_write_tokens`, `cox.usage.estimated`, `cox.cost.usd`, `cox.provider.call.ordinal`, `cox.retry.count` |
| `execute_tool` | one tool call | `gen_ai.tool.name`, `gen_ai.tool.call.id`, `cox.tool.subject`, `cox.tool.risk`, `cox.tool.duration_ms`, `cox.tool.output_bytes`, `cox.tool.success`, `cox.archive.id` |

A failed span sets `error.type` and `otel.status_code=ERROR`. `cox.archive.id`
is the id `cox expand <id>` takes, so a truncated tool result in a trace can
still be read in full locally.

### Content attributes (opt-in only)

These four are written only under
`OTEL_INSTRUMENTATION_GENAI_CAPTURE_MESSAGE_CONTENT=true`:
`gen_ai.input.messages`, `gen_ai.output.messages`,
`gen_ai.tool.call.arguments` and `gen_ai.tool.call.result`. A contract test
asserts they are absent by default and present after the opt-in, so the
default cannot regress silently.

## Backends

Every backend below speaks OTLP/HTTP; only the endpoint and headers change.

**Local smoke stack (Collector + Jaeger + Grafana/Tempo).**

```bash
docker compose -f docker-compose.telemetry.yml up -d
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318 \
  cox --set telemetry.otel=true -p "list the files in this repository"
open http://localhost:16686           # Jaeger: service "cox"
open http://localhost:3000/explore    # Grafana → Tempo → search service.name=cox
docker compose -f docker-compose.telemetry.yml logs otel-collector | grep -A3 LogRecord
docker compose -f docker-compose.telemetry.yml down -v
```

The collector receives on 4317/4318, fans traces out to Jaeger and Tempo
and prints log records to its own stdout; nothing is persisted after
`down -v`.

**Jaeger alone** (v2 accepts OTLP directly):
`docker run --rm -p 16686:16686 -p 4318:4318 jaegertracing/jaeger:2.2.0`,
then the same `OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318`.

**SigNoz** (self-hosted or cloud): point at the SigNoz collector,
`OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318` for the docker install,
or `https://ingest.<region>.signoz.cloud:443` with
`OTEL_EXPORTER_OTLP_HEADERS=signoz-ingestion-key=<key>` for cloud.

**Grafana Cloud / Tempo**: `OTEL_EXPORTER_OTLP_ENDPOINT=https://otlp-gateway-<region>.grafana.net/otlp`
with `OTEL_EXPORTER_OTLP_HEADERS=Authorization=Basic <base64 instance:token>`.

**Honeycomb, Maple and other hosted collectors**: the vendor's OTLP/HTTP
endpoint plus its API-key header, for example
`OTEL_EXPORTER_OTLP_ENDPOINT=https://api.honeycomb.io` and
`OTEL_EXPORTER_OTLP_HEADERS=x-honeycomb-team=<key>`.

## Reading a trace

A session is one trace; open it and you see the turn, under it one span per
provider round with the model, token counts and cost, and under each round
the tool calls it made with their subjects, durations and outcomes. Errors
mark the span status; the `Notice` events cox shows in the TUI are log
records attached to the same trace, so a permission refusal or a budget
warning sits next to the round that caused it.
