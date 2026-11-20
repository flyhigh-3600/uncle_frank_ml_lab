// Browser-local copies of repository examples; syntax follows docs/grammar.ebnf.
export const samples = {
  "text_io.ufp": `// examples/text_io.ufp — text terminals and a direct pipe.
input prompt = """
Why did the tensor cross the road?
""";

output reply;

pipe prompt.text -> reply.text;`,
  "portal_demo.ufp": `// examples/portal_demo.ufp — portal with annotated nodes and pipes.
portal qwen = "./qwen3_4b.ufp" as Qwen_3_4b;

@T(0)
node encode : tokenizer.encode(text = "hello, frank");

@T(0.1)
node generate : Qwen_3_4b(prompt = encode.out, max_tokens = 1024);

@T(1)
node decode : tokenizer.decode(tokens = generate.out);

pipe encode.out -> generate.prompt;
pipe generate.out -> decode.tokens;`,
  "model_layers.ufp": `// examples/model_layers.ufp — portal plus text terminals.
portal layer_000 = "./layers/layer_000.ufp" as main;

input prompt = "inspect this layer tree";
output reply;

// A later tokenizer/decoder chain connects prompt -> layer_000 -> reply.`,
  "layer_000.ufp": `// examples/layers/layer_000.ufp — inspectable model layer.
matrix[3,4] layer_0 = [
    [0.30, 0.45, 0.12, 0.91],
    [0.08, 0.22, 0.77, 0.19],
    [0.64, 0.51, 0.03, 0.38]
];

procedure main(
    input  matrix<2>[1,3] of float<32> x,
    output matrix<2>[1,4] of float<32> y
) {
    y = x * layer_0;
    return y;
}`,
  "notes.txt": `This file is a .txt. It does absolutely nothing.

It is not compiled. It is not imported. It is not type-checked.
It is where the lab notes live: half-finished ideas, warnings from the
night shift, and the occasional apology to the specimens.

— F.`
};

export function assertSamples() {
  const terminals = samples["text_io.ufp"];
  if (!/\binput\s+prompt\s*=\s*"""[\s\S]*?"""\s*;/.test(terminals)) throw new Error("missing grammar-shaped input terminal");
  if (!/\boutput\s+reply\s*;/.test(terminals)) throw new Error("missing grammar-shaped output terminal");
  if (!/\bpipe\s+prompt\.text\s*->\s*reply\.text\s*;/.test(terminals)) throw new Error("missing text terminal pipe");
  if (!/\bportal\s+\w+\s*=\s*"/.test(samples["portal_demo.ufp"]) ||
      !/\bmatrix(?:<\d+>)?\s*\[[\w,\s]+\]\s+\w+\s*=/.test(samples["layer_000.ufp"])) throw new Error("portal/layer sample syntax is invalid");
  if (/\b(?:input|output|pipe)\b/.test(samples["notes.txt"])) throw new Error("TXT sample must remain inert");
}
