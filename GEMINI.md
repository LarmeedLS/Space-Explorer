## 1. Context and the goal of the project
- This project is about space exploration with relativistic effect.
- The simulation should be playable on high-end hardware but also lower tier (like <16GB RAM, <4GB VRAM, no dGPU). As this project may likely be shared to other people that have worse computer than this system.

## 2. How I test
- The output of `./run.sh` (the script to start the game) is always in `/tmp/test-space.txt`, in case the file doesn't exist, read in `./tmp`. This file is useful to see if the game is executed properly.
- There are screenshots from the game where there can be bugs present, they are placed in `tmp/screenshots`, when needed, look at the pictures in this directory.
- I test this application at 165Hz, CPU Over 2.4GHz up to 5.3Ghz.

## 3. Autonomous Execution & Destructive Action Policy
- **Destructive Deletion Ban:** Never execute `rm`, `shred`, `wipefs`, `dd`, or directory truncations (`> file`) automatically. If file deletion or purging is strictly required, ask the user or implement a safe trash pattern (e.g., moving to a `.trash` or backup directory).
- **Autonomous Loop:** You are authorized to create files, edit code, run build tools, run test suites, and read diagnostic outputs iteratively until the requested task is complete or a blocking error is encountered.
- **File Replacements & Cleanup:** When cleaning build artifacts or temporary files, place them in a dedicated `./tmp` or trash directory inside the project root, or explicitly ask the user before deleting.

## 4. Asset & Skin Repository
- High-resolution textures and source planet maps are stored in `tmp/skin/`.
- Active runtime textures are in `assets/textures/planets/`.

## 5. Permissions
- When using `cp`, `mv`, or similar commands about file operations outside of editing or creating files of the project, you have to ask the user's permission, as sandbox blocks these kind of commands.
- If you use commands that don't modify files and see this message "Caused by: Read-only file system (os error 30)" or similar errors, if it's needed for the project and there are no other ways, ask the user's permission to be able to get a result.
- Since you are run in a sandbox, to use cargo properly add `RUSTC_WRAPPER=""` before the commands.
