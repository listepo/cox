// What the user asks a session to do (DT§4.3 Intents), as cox-ffi's
// `Intent`. Separate from the timeline because it only flows the other way:
// the stores send it, nothing decodes it.

public enum Intent: Equatable, Sendable {
    case send(text: String, attachments: [Attachment])
    case approve(call: String, decision: Decision)
    case answer(question: String, text: String?)
    case interrupt
    case queue(text: String)
    case compact(focus: String?)
    case setMode(mode: PermissionMode)
    case switchModel(tier: Tier, model: String?)
    case setEffort(effort: Effort?)
    case rewind(toTurn: UInt32, code: Bool, conversation: Bool)
    case redo
    case fork(turn: UInt32?)
    case handoff(objective: String)
    case background(call: String)
    case shell(command: String, share: Bool)
    case command(line: String)
}

public struct Attachment: Equatable, Sendable {
    public var name: String
    public var mediaType: String
    public var dataB64: String

    public init(name: String, mediaType: String, dataB64: String) {
        (self.name, self.mediaType, self.dataB64) = (name, mediaType, dataB64)
    }
}

public enum PermissionMode: Equatable, Sendable { case `default`, plan, auto, bypass }

public enum Effort: Equatable, Sendable { case low, medium, high, xhigh }
