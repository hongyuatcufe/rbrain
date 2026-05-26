# Pending Fixes

## 1. think 提示词引用格式
**问题**: `rbrain think` 提示词要求 `[[slug]]`，但 context 块里已经有 `chunk:N` 信息，LLM 有时自发用 `[[slug | chunk:N]]`，有时只用 `[[slug]]`，行为不一致。  
**修复**: 把 think 的中英文 system prompt 里的引用格式改成 `[[slug | chunk:N]]`，与 generate_wiki 对齐。  
**文件**: `crates/rbrain-engine/src/engine.rs` think() 函数，~1683 行和 ~1694 行。

## 2. find_chunk_id_for_context 匹配率偏低
**问题**: LLM 返回的 `context` 字段是"相关文本片段"，但 LLM 经常改写而非原文引用，导致 `text.contains(&needle)` 失败，chunk_id 退回 -1。  
**建议**: needle 从 60 字符缩短到 30 字符，提高容错率。  
**文件**: `crates/rbrain-engine/src/engine.rs` find_chunk_id_for_context()，~2241 行。
