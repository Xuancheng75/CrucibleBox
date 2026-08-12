declare module 'sql.js' {
  interface SqlJsStatic {
    Database: new (data?: ArrayLike<number> | Buffer | null) => Database
  }

  interface QueryExecResult {
    columns: string[]
    values: unknown[][]
  }

  interface Statement {
    bind(params?: unknown[]): boolean
    step(): boolean
    getAsObject(params?: object): Record<string, unknown>
    free(): boolean
    reset(): void
  }

  interface Database {
    run(sql: string, params?: unknown[]): Database
    exec(sql: string): QueryExecResult[]
    prepare(sql: string): Statement
    export(): Uint8Array
    close(): void
  }

  export type { Database, Statement, QueryExecResult }

  export default function initSqlJs(config?: unknown): Promise<SqlJsStatic>
}
