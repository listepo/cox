//! Built-in tools and the sandbox (Seatbelt, Landlock/bwrap): read, edit,
//! write, bash, grep, glob, outline, web, todo, ask_user, agent. Separate
//! from `cox-core` because every tool touches the filesystem or a process
//! and must go through a trait, never called directly by the loop.
