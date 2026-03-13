export type Message = {
  id: string
  role: "user" | "assistant"
  content: string
  timestamp: Date
}

export type Conversation = {
  id: string
  title: string
  updatedAt: Date
  messages: Message[]
}

function time(daysAgo: number, hours: number, minutes: number): Date {
  const d = new Date()
  d.setDate(d.getDate() - daysAgo)
  d.setHours(hours, minutes, 0, 0)
  return d
}

export const conversations: Conversation[] = [
  {
    id: "1",
    title: "Refactoring the auth module",
    updatedAt: time(0, 14, 32),
    messages: [
      {
        id: "1-1",
        role: "user",
        content:
          "I need to refactor the authentication module. The current implementation has JWT validation, session management, and role-based access control all mixed together in a single file. Can you suggest a clean architecture?",
        timestamp: time(0, 14, 20),
      },
      {
        id: "1-2",
        role: "assistant",
        content:
          "Here's a clean separation for the auth module:\n\n**1. `auth/jwt.rs`** — Token creation, validation, and claims extraction\n\n**2. `auth/session.rs`** — Session lifecycle, storage, and cleanup\n\n**3. `auth/rbac.rs`** — Role definitions, permission checks, middleware\n\n**4. `auth/mod.rs`** — Public API surface, re-exports\n\nThe key principle is single responsibility. Each file handles one concern. The `mod.rs` acts as the facade, exposing only what external modules need.\n\nWould you like me to start with the JWT extraction?",
        timestamp: time(0, 14, 21),
      },
      {
        id: "1-3",
        role: "user",
        content:
          "Yes, let's start with JWT. The current code mixes validation with database lookups for token revocation. How should I handle that?",
        timestamp: time(0, 14, 25),
      },
      {
        id: "1-4",
        role: "assistant",
        content:
          'Good catch — token revocation is a cross-cutting concern. Here\'s my recommendation:\n\n```rust\n// auth/jwt.rs — Pure JWT logic, no DB dependency\npub struct JwtValidator {\n    secret: Secret,\n    issuer: String,\n}\n\nimpl JwtValidator {\n    pub fn validate(&self, token: &str) -> Result<Claims, AuthError> {\n        // Pure cryptographic validation only\n    }\n}\n```\n\n```rust\n// auth/revocation.rs — Token revocation with storage\npub struct RevocationChecker {\n    store: Arc<dyn RevocationStore>,\n}\n\nimpl RevocationChecker {\n    pub async fn is_revoked(&self, jti: &str) -> Result<bool, AuthError> {\n        self.store.check(jti).await\n    }\n}\n```\n\nThe `JwtValidator` stays pure — no I/O, no async, easily testable. Revocation becomes a separate check that the middleware composes. This way you can unit test JWT validation without mocking databases.',
        timestamp: time(0, 14, 27),
      },
      {
        id: "1-5",
        role: "user",
        content:
          "That's exactly what I needed. Let me implement this and I'll come back if I run into issues with the middleware composition.",
        timestamp: time(0, 14, 32),
      },
    ],
  },
  {
    id: "2",
    title: "Database migration strategy",
    updatedAt: time(0, 11, 15),
    messages: [
      {
        id: "2-1",
        role: "user",
        content:
          "We need to add a new column to the users table in production. What's the safest migration strategy for zero downtime?",
        timestamp: time(0, 11, 10),
      },
      {
        id: "2-2",
        role: "assistant",
        content:
          "For zero-downtime migrations, use the **expand-and-contract** pattern:\n\n**Phase 1 — Expand:** Add the new column as nullable with no default. This is a non-blocking DDL operation in most databases.\n\n**Phase 2 — Migrate:** Backfill existing rows in batches. Write new code that populates the column for new records.\n\n**Phase 3 — Contract:** Once all rows are populated and the application fully uses the new column, add the NOT NULL constraint and clean up.\n\nThis avoids table locks and allows rollback at each phase. Each phase is independently deployable and reversible.",
        timestamp: time(0, 11, 12),
      },
    ],
  },
  {
    id: "3",
    title: "API rate limiting design",
    updatedAt: time(1, 16, 45),
    messages: [
      {
        id: "3-1",
        role: "user",
        content:
          "What's the best approach for API rate limiting in a distributed system?",
        timestamp: time(1, 16, 40),
      },
      {
        id: "3-2",
        role: "assistant",
        content:
          "A **sliding window counter** using Redis is the most practical approach for production distributed systems. It provides a good balance between accuracy and performance.\n\nThe algorithm works by maintaining a counter for the current and previous time windows, then interpolating between them based on elapsed time. This gives sub-second accuracy without the memory overhead of a true sliding log.\n\nFor your stack, I'd recommend the `redis-cell` module which implements GCRA (Generic Cell Rate Algorithm). It's atomic, efficient, and handles distributed rate limiting out of the box — no Lua scripts needed.",
        timestamp: time(1, 16, 42),
      },
    ],
  },
  {
    id: "4",
    title: "CI/CD pipeline setup",
    updatedAt: time(1, 10, 30),
    messages: [],
  },
  {
    id: "5",
    title: "Error handling patterns in Rust",
    updatedAt: time(3, 15, 20),
    messages: [],
  },
  {
    id: "6",
    title: "WebSocket reconnection logic",
    updatedAt: time(5, 9, 15),
    messages: [],
  },
]
