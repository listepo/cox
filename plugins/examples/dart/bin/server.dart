// The Dart variant of cox's shared plugin example (PL§13, T33.38): one
// MCP tool, `count`, served over stdio by dart_mcp. Every other language's
// example wires a status segment, a hook and a command through the wasm
// ABI; Dart cannot emit a wasm module extism can load (T33.37,
// research.md §4.3.5 P44), so this is the one thing left for it to show:
// a plugin whose only capability is an `[[mcp]]` server (`plugin.toml`).

import 'dart:async';
import 'dart:io' as io;

import 'package:dart_mcp/server.dart';
import 'package:dart_mcp/stdio.dart';

void main() {
  CountServer(stdioChannel(input: io.stdin, output: io.stdout));
}

/// Exposes one tool, `count`, over MCP. `plugin.toml`'s `[[mcp]] name` is
/// also `count`, so cox's tool dispatch sees it as
/// `mcp__example-dart-count__count`.
base class CountServer extends MCPServer with ToolsSupport {
  CountServer(super.channel)
    : super.fromStreamChannel(
        implementation: Implementation(
          name: 'cox Dart MCP example',
          version: '0.1.0',
        ),
        instructions: 'Call count to see how many times it has been called.',
      ) {
    registerTool(countTool, _count);
  }

  /// Calls made to this server process so far.
  int _calls = 0;

  final countTool = Tool(
    name: 'count',
    description:
        'Increments and returns how many times this tool has been called '
        'in this server process.',
    inputSchema: Schema.object(properties: {}),
  );

  FutureOr<CallToolResult> _count(CallToolRequest request) async {
    _calls += 1;
    return CallToolResult(content: [TextContent(text: '$_calls')]);
  }
}
