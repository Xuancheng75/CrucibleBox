declare module '@iarna/toml/parse-string.js' {
  export default function parseToml(input: string): unknown
}

declare module '@iarna/toml/stringify.js' {
  export default function stringifyToml(input: unknown): string
}
