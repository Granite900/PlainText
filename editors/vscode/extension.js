//@ts-check
"use strict";

const vscode = require("vscode");
const { LanguageClient, TransportKind } = require("vscode-languageclient/node");

/** @type {LanguageClient | undefined} */
let client;

/**
 * @param {vscode.ExtensionContext} context
 */
async function activate(context) {
  const config = vscode.workspace.getConfiguration("plaintext");
  const command = config.get("path") || "plaintext";

  const serverOptions = {
    run: { command, args: ["lsp"], transport: TransportKind.stdio },
    debug: { command, args: ["lsp"], transport: TransportKind.stdio },
  };

  const clientOptions = {
    documentSelector: [{ scheme: "file", language: "pt" }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.pt"),
    },
  };

  client = new LanguageClient(
    "plaintext",
    "PlainText Language Server",
    serverOptions,
    clientOptions
  );

  try {
    await client.start();
  } catch (err) {
    const msg = err && err.message ? err.message : String(err);
    vscode.window.showErrorMessage(
      `PlainText language server failed to start (is \`${command}\` on your PATH?). ${msg}`
    );
  }

  context.subscriptions.push({
    dispose: () => {
      if (client) {
        return client.stop();
      }
    },
  });
}

function deactivate() {
  if (!client) {
    return undefined;
  }
  return client.stop();
}

module.exports = { activate, deactivate };
