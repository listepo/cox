// Generated cox-ffi values ⇄ `CoxClient` values, field for field. The two
// sets have the same shape (both mirror cox-app's serde types); this file
// keeps the generated ones inside CoxCore, so the stores and views never
// link Rust. Exhaustive switches: a variant added in Rust fails this build.

import CoxClient
import CoxFFIBindings

extension CoxClient.Block {
    init(_ b: CoxFFIBindings.Block) { self.init(id: b.id, turn: b.turn, kind: .init(b.kind)) }
}

extension CoxClient.BlockKind {
    init(_ k: CoxFFIBindings.BlockKind) {
        switch k {
        case let .user(text, attachments): self = .user(text: text, attachments: attachments)
        case let .assistant(text, doc): self = .assistant(text: text, doc: .init(doc))
        case let .thinking(text): self = .thinking(text: text)
        case let .tool(tool, summary, icon, risk, state, tail, archive, diff, durationMs):
            self = .tool(
                tool: tool, summary: summary, icon: .init(icon), risk: .init(risk), state: .init(state),
                tail: tail, archive: archive.map { .init(id: $0.id, bytes: $0.bytes) },
                diff: diff.map { .init(path: $0.path, unified: $0.unified) }, durationMs: durationMs)
        case let .toolGroup(summary, children, state):
            self = .toolGroup(summary: summary, children: children, state: .init(state))
        case let .approval(call, tool, summary, why, source, decision, by):
            self = .approval(
                call: call, tool: tool, summary: summary, why: .init(why),
                source: source.map { .init(session: $0.session, agent: $0.agent, preset: $0.preset) },
                decision: decision.map { CoxClient.Decision($0) }, by: by.map { CoxClient.DecidedBy($0) })
        case let .question(call, question, options, answer):
            self = .question(call: call, question: question, options: options, answer: answer)
        case let .task(task, label, tier, done, costUsd, exitCode):
            self = .task(
                task: task, label: label, tier: .init(tier), done: done, costUsd: costUsd,
                exitCode: exitCode)
        case let .compaction(before, after, reason, summary):
            self = .compaction(
                beforeTokens: before, afterTokens: after, reason: .init(reason), summary: summary)
        case let .checkpoint(files): self = .checkpoint(files: files)
        case let .notice(level, text): self = .notice(level: .init(level), text: text)
        case let .error(text, fatal): self = .error(text: text, fatal: fatal)
        case let .turnMeta(model, tier, usage, stop):
            self = .turnMeta(
                model: model, tier: .init(tier), usage: usage.map { CoxClient.Usage($0) },
                stop: stop.map { CoxClient.StopReason($0) })
        }
    }
}

extension CoxClient.TimelinePatch {
    init(_ p: CoxFFIBindings.TimelinePatch) {
        switch p {
        case let .reset(blocks): self = .reset(blocks: blocks.map { CoxClient.Block($0) })
        case let .upsert(block, after): self = .upsert(block: .init(block), after: after)
        case let .appendText(id, text): self = .appendText(id: id, text: text)
        case let .docTail(id, from, blocks):
            self = .docTail(id: id, from: from, blocks: blocks.map { CoxClient.DocBlock($0) })
        case let .remove(id): self = .remove(id: id)
        case let .usage(usage): self = .usage(usage: .init(usage))
        }
    }
}

extension CoxClient.StyledDoc {
    init(_ d: CoxFFIBindings.StyledDoc) { self.init(blocks: d.blocks.map { CoxClient.DocBlock($0) }) }
}

extension CoxClient.DocBlock {
    init(_ b: CoxFFIBindings.DocBlock) {
        let spans = { (lines: [[CoxFFIBindings.Span]]) in lines.map { $0.map { CoxClient.Span($0) } } }
        switch b {
        case let .text(kind, lines): self = .text(kind: .init(kind), lines: spans(lines))
        case let .code(lang, lines): self = .code(lang: lang, lines: spans(lines))
        case let .table(rows): self = .table(rows: rows)
        case .rule: self = .rule
        }
    }
}

extension CoxClient.TextKind {
    init(_ k: CoxFFIBindings.TextKind) {
        switch k {
        case .paragraph: self = .paragraph
        case .heading(let level): self = .heading(level)
        case .list: self = .list
        case .quote: self = .quote
        }
    }
}

extension CoxClient.Span {
    init(_ s: CoxFFIBindings.Span) {
        self.init(text: s.text)
        (token, rgb, link) = (.init(s.token), s.rgb, s.link)
        (bold, italic, strike, underline) = (s.bold, s.italic, s.strike, s.underline)
    }
}

extension CoxClient.StyleToken {
    init(_ t: CoxFFIBindings.StyleToken) {
        switch t {
        case .text: self = .text
        case .dim: self = .dim
        case .accent: self = .accent
        case .user: self = .user
        case .agent: self = .agent
        case .tool: self = .tool
        case .ok: self = .ok
        case .warn: self = .warn
        case .error: self = .error
        case .diffAdd: self = .diffAdd
        case .diffDel: self = .diffDel
        case .diffHunk: self = .diffHunk
        case .border: self = .border
        case .selection: self = .selection
        }
    }
}

extension CoxClient.Icon {
    init(_ i: CoxFFIBindings.Icon) {
        switch i {
        case .read: self = .read
        case .edit: self = .edit
        case .shell: self = .shell
        case .search: self = .search
        case .web: self = .web
        case .todo: self = .todo
        case .ask: self = .ask
        case .agent: self = .agent
        case .mcp: self = .mcp
        case .tool: self = .tool
        }
    }
}

extension CoxClient.Risk {
    init(_ r: CoxFFIBindings.Risk) {
        switch r {
        case .readOnly: self = .readOnly
        case .write: self = .write
        case .exec: self = .exec
        case .destructive: self = .destructive
        }
    }
}

extension CoxClient.ToolState {
    init(_ s: CoxFFIBindings.ToolState) {
        switch s {
        case .running: self = .running
        case .done: self = .done
        case .failed: self = .failed
        }
    }
}

extension CoxClient.Tier {
    init(_ t: CoxFFIBindings.Tier) {
        switch t {
        case .cheap: self = .cheap
        case .code: self = .code
        case .think: self = .think
        }
    }
}

extension CoxFFIBindings.Tier {
    init(_ t: CoxClient.Tier) {
        switch t {
        case .cheap: self = .cheap
        case .code: self = .code
        case .think: self = .think
        }
    }
}

extension CoxClient.Level {
    init(_ l: CoxFFIBindings.Level) {
        switch l {
        case .info: self = .info
        case .warn: self = .warn
        case .budget: self = .budget
        case .security: self = .security
        }
    }
}

extension CoxClient.DecidedBy {
    init(_ d: CoxFFIBindings.DecidedBy) {
        switch d {
        case .user: self = .user
        case .rule: self = .rule
        case .session: self = .session
        case .policy: self = .policy
        case .hook: self = .hook
        }
    }
}

extension CoxClient.CompactReason {
    init(_ r: CoxFFIBindings.CompactReason) {
        switch r {
        case .preCall: self = .preCall
        case .postTurn: self = .postTurn
        case .manual: self = .manual
        case .contextTooLong: self = .contextTooLong
        }
    }
}

extension CoxClient.Why {
    init(_ w: CoxFFIBindings.Why) {
        switch w {
        case let .ruleAsk(rule): self = .ruleAsk(rule: rule)
        case let .risk(risk): self = .risk(risk: .init(risk))
        case let .sandboxDenied(detail): self = .sandboxDenied(detail: detail)
        case let .policy(policy):
            switch policy {
            case .untrusted: self = .policy(policy: .untrusted)
            case .onRequest: self = .policy(policy: .onRequest)
            case .onFailure: self = .policy(policy: .onFailure)
            case .never: self = .policy(policy: .never)
            }
        }
    }
}

extension CoxClient.Decision {
    init(_ d: CoxFFIBindings.Decision) {
        switch d {
        case .allow: self = .allow
        case .allowForSession: self = .allowForSession
        case let .deny(reason): self = .deny(reason: reason)
        case let .edit(input): self = .edit(input: input)
        }
    }
}

extension CoxFFIBindings.Decision {
    init(_ d: CoxClient.Decision) {
        switch d {
        case .allow: self = .allow
        case .allowForSession: self = .allowForSession
        case let .deny(reason): self = .deny(reason: reason)
        case let .edit(input): self = .edit(input: input)
        }
    }
}

extension CoxClient.StopReason {
    init(_ s: CoxFFIBindings.StopReason) {
        switch s {
        case .endTurn: self = .endTurn
        case .maxTurns: self = .maxTurns
        case .interrupted: self = .interrupted
        case .budget: self = .budget
        case let .refusal(detail): self = .refusal(detail: detail)
        case .error: self = .error
        }
    }
}

extension CoxClient.Usage {
    init(_ u: CoxFFIBindings.Usage) {
        self.init(
            inputTokens: u.inputTokens, outputTokens: u.outputTokens,
            cacheReadTokens: u.cacheReadTokens, cacheWriteTokens: u.cacheWriteTokens,
            estimated: u.estimated, costUsd: u.costUsd, latencyMs: u.latencyMs)
    }
}

extension CoxClient.UsageView {
    init(_ v: CoxFFIBindings.UsageView) {
        self.init(
            session: .init(v.session), turn: v.turn.map { CoxClient.TurnUsage($0) },
            contextTokens: v.contextTokens)
    }
}

extension CoxClient.Tally {
    init(_ t: CoxFFIBindings.Tally) {
        self.init(
            sent: t.sent, received: t.received, cacheRead: t.cacheRead, cacheWrite: t.cacheWrite,
            uncached: t.uncached, costUsd: t.costUsd, calls: t.calls, estimated: t.estimated)
    }
}

extension CoxClient.TurnUsage {
    init(_ t: CoxFFIBindings.TurnUsage) {
        self.init(
            turn: t.turn, tally: .init(t.tally), thinkingTokens: t.thinkingTokens, ttftMs: t.ttftMs,
            tokPerS: t.tokPerS, exact: t.exact, sparkline: t.sparkline, done: t.done)
    }
}

extension CoxFFIBindings.Intent {
    init(_ i: CoxClient.Intent) {
        switch i {
        case let .send(text, attachments):
            self = .send(
                text: text,
                attachments: attachments.map {
                    .init(name: $0.name, mediaType: $0.mediaType, dataB64: $0.dataB64)
                })
        case let .approve(call, decision): self = .approve(call: call, decision: .init(decision))
        case let .answer(question, text): self = .answer(question: question, text: text)
        case .interrupt: self = .interrupt
        case let .queue(text): self = .queue(text: text)
        case let .compact(focus): self = .compact(focus: focus)
        case let .setMode(mode):
            switch mode {
            case .default: self = .setMode(mode: .default)
            case .plan: self = .setMode(mode: .plan)
            case .auto: self = .setMode(mode: .auto)
            case .bypass: self = .setMode(mode: .bypass)
            }
        case let .switchModel(tier, model): self = .switchModel(tier: .init(tier), model: model)
        case let .setEffort(effort):
            switch effort {
            case nil: self = .setEffort(effort: nil)
            case .low: self = .setEffort(effort: .low)
            case .medium: self = .setEffort(effort: .medium)
            case .high: self = .setEffort(effort: .high)
            case .xhigh: self = .setEffort(effort: .xhigh)
            }
        case let .rewind(toTurn, code, conversation):
            self = .rewind(toTurn: toTurn, code: code, conversation: conversation)
        case .redo: self = .redo
        case let .fork(turn): self = .fork(turn: turn)
        case let .handoff(objective): self = .handoff(objective: objective)
        case let .background(call): self = .background(call: call)
        case let .shell(command, share): self = .shell(command: command, share: share)
        case let .command(line): self = .command(line: line)
        }
    }
}
