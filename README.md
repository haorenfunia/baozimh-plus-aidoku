# 包子漫画 Plus for Aidoku

这是一个 Aidoku 0.7+ 新格式中文源仓库，仅提供包子漫画 Plus 直连源，不包含 Proxy 源。

## 添加到 Aidoku

在 Aidoku 中添加下面的源列表地址：

```text
https://raw.githubusercontent.com/haorenfunia/baozimh-plus-aidoku/main/public/index.min.json
```

如果 raw.githubusercontent.com 访问不稳定，可以使用 jsDelivr：

```text
https://cdn.jsdelivr.net/gh/haorenfunia/baozimh-plus-aidoku@main/public/index.min.json
```

当前源：

- 包子漫画 Plus（直连）

源码位于 `sources/zh.baozimh1`，推送后由 GitHub Actions 自动打包。源内置 Plus 的 App 最新章节备用节点和 App 章节链接解析。

## v11 章节记录修复

章节 ID 不再包含会随更新而消失的 `|latest` 后缀，避免上一话从最新章变为普通章时丢失已读匹配。旧 ID 通过 Aidoku 的 `MigrationHandler` 迁移，漫画 ID 保持不变。

更新前请先创建 Aidoku 备份。更新时若提示源有重大变更，请选择“迁移”，然后刷新漫画目录。旧记录若仍存在，可映射到固定 ID；已经被删除的历史无法仅靠源恢复。全章节 App 节点请求和图片处理保持不变。

可独立验证 Rust/WASM 的旧 ID 迁移：

```sh
node sources/zh.baozimh1/tests/chapter_keys_wasm.mjs
```

需要 Rust 的 `wasm32-unknown-unknown` 目标。完整目录解析和源接口回归测试位于 `src/html/mod.rs` 和 `src/lib.rs`，需使用 Aidoku 测试运行环境执行。
