// Deterministic keyless OpenAI-compatible model for the disposable desktop.
// Does not record prompts or use external services.
import { createServer } from "node:http";
export async function startProvider() {
  const server = createServer(async (req, res) => {
    if (req.url === "/v1/models") {
      res.setHeader("content-type", "application/json");
      return res.end(
        JSON.stringify({ data: [{ id: "qa-model", object: "model" }] }),
      );
    }
    if (req.url !== "/v1/chat/completions") {
      res.writeHead(404);
      return res.end();
    }
    let bytes = "";
    for await (const chunk of req) bytes += chunk;
    const body = JSON.parse(bytes);
    const tool = body.tools?.find((t) =>
      ["submit_findings", "submit_review"].includes(t.function.name),
    );
    let chunks = [
      [{ role: "assistant", content: "A triangle has three sides." }, null],
      [{}, "stop"],
    ];
    if (tool) {
      const messageId = JSON.stringify(body.messages).match(
        /msg_[a-z0-9]+_[0-9a-f]{8}/,
      )?.[0];
      const result =
        tool.function.name === "submit_findings"
          ? {
              propositions: [
                {
                  id: "p1",
                  name: "Triangle",
                  statement: "A triangle has three sides.",
                  aliases: [],
                  messageId,
                },
                {
                  id: "p2",
                  name: "Boundary",
                  statement: "Its boundary comprises three segments.",
                  aliases: [],
                  messageId,
                },
                { id: "p3", name: "Closed boundary", statement: "The segments form a closed boundary.", aliases: [], messageId },
              ],
              chains: [
                { premises: ["p1"], conclusion: "p2" },
                { premises: ["p1", "p2"], conclusion: "p3" },
              ],
            }
          : { links: [] };
      chunks = [
        [
          {
            role: "assistant",
            tool_calls: [
              {
                index: 0,
                id: "qa-call",
                type: "function",
                function: {
                  name: tool.function.name,
                  arguments: JSON.stringify(result),
                },
              },
            ],
          },
          null,
        ],
        [{}, "tool_calls"],
      ];
    }
    res.setHeader("content-type", "text/event-stream");
    for (const [delta, reason] of chunks) {
      res.write(
        "data: " +
          JSON.stringify({
            id: "qa",
            object: "chat.completion.chunk",
            created: 1,
            model: "qa-model",
            choices: [{ index: 0, delta, finish_reason: reason }],
          }) +
          "\n\n",
      );
    }
    res.end("data: [DONE]\n\n");
  });
  await new Promise((resolve) => server.listen(9401, "127.0.0.1", resolve));
  return server;
}
