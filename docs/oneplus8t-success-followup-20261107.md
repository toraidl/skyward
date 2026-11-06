# OnePlus 8T 解密成功后的日志复核

## 取证与已验证结果

2026-11-07，通过 Wi-Fi ADB 确认设备为 KB2000 / 50a390b0、槽位 `_a`，取回用户保存的 `/sdcard/skyward/logs/20261107-004722/`，以及自动持久化的 `/mnt/vendor/oplusreserve/skyward/current/`。主机副本在 `~/skyward-work/logs-20261107-success/`。

用户报告解密成功；日志进一步显示：

- 启动后 19.824 秒，`/dev/block/dm-15` 的 F2FS 检查退出码为 0，`/data` 挂载完成，仍锁定的用户列表为空。
- UI 截图显示 `Decrypted`；用户文件中的 OTA 被识别，之后由用户取消。不能把这次操作算作 OTA 安装验收。
- 自动日志位于 op2 上，回到正常系统后仍能拉取，证明此次 recovery 的持久日志服务正常工作。
- 设备 recovery_a 的 SHA-256 仍为已成功的 `6c5323bb9bc0c83a024aeb94eed326f30110b00e2f30158e7524bc0a628ffa33`。

这些证明当前系统与用户数据路径已可用，不代替对不同锁屏凭据、其他 ROM、备份恢复、刷写或 USB 实际传输的独立验收。

## 发现与已落实的源码修正

### 1. 左上挖孔遮挡时间

原截图中时间墨迹范围为 `x=114..225, y=60..92`。正常系统 DisplayCutout 报告左上安全区 `Rect(0,0-165,103)`，时间确实落入该区域。

`52bfa81` 增加设备配置 `display.status_bar_left_inset`，kebab 设为 189 个物理像素，即安全区右侧额外留 24 像素。只移动状态栏时间，不移动页面或右侧图标；常规页面与转场共用配置。其他机型默认 0，保持原布局。

`d62baf8` 为主机预览加入 `--config` 和 `--home`。本次 1080×2400 预览的时间墨迹为 `x=191..301, y=60..92`，已完全避开该安全区。预览在 `~/skyward-work/logs-20261107-success/ui-preview/home.png`；物理挖孔不在软件截图中，实屏效果仍需下一次试机确认。

### 2. logcat 无法取日志：ramdisk 缺少 logd

持久日志的 `logcat.log` 只有 `Logcat read failure: No such file or directory`。成功镜像的 ramdisk 声明了 `service logd`，实际没有 `/system/bin/logd`；进程快照也没有 logd。`logd.ready=true` 单独不能证明服务可用。

`0c909b4` 为打包器增加 `--logd` 和可重复的 `--library`：只补缺失文件，拒绝覆盖已有 ramdisk 库，并检查输入为小端 arm64 ELF。补入 logd 时增加独立的 boot 启动动作，使其不依赖 USB 配置触发。

本次候选的 logd 来自本地 OrangeFox 12.1 构建产物 `~/ofox_12.1/out/target/product/opkona/recovery/root/system/bin/logd`，SHA-256 为 `2785b8854eba1778e884a72991cfad23c403bbbf5c86bc5f039b766c6fa130ba`。未采用 Android 17 的 logd 替换旧版 recovery 组件。

### 3. gatekeeperd 与 cryptoeng 反复退出：存在确定的链接依赖缺口

内核日志记录 cryptoeng 约每 5 秒退出一次，状态快照中 `gatekeeperd` 和 `hal_cryptoeng_oplus` 都为 restarting；底层 HIDL Gatekeeper HAL 是另一个进程，其状态为 running。

原厂 `system/etc/ld.config.txt` 的 recovery namespace 只搜索 `/system/${LIB}`。两个程序的 DT_NEEDED 在该目录缺少：

| 程序 | 缺少的库 |
| --- | --- |
| gatekeeperd | `libsecurity_event_dcs.so` |
| vendor.oplus.hardware.cryptoeng@1.0-service | `libsecurity_event_dcs_vnd.so`、`vendor.oplus.hardware.cryptoeng-V1-ndk_platform.so`、`vendor.oplus.hardware.biometrics.fingerprintpay-V1-ndk_platform.so` |

候选从该机的 `/odm/lib64/` 补入上述四个文件，未替换其他库。包含 logd、两个程序及传递依赖共 69 个 ELF 的静态核对，未发现缺少的 SONAME 或未解析的带版本强引用符号。完整库清单和来源哈希位于 `~/skyward-work/support-20261107/manifest.json`，依赖检查在日志目录的 `native/support-dependency-audit.json`。

这消除了已确认的链接缺口；程序后续的 IPC、固件或业务初始化是否正常仍待 recovery 运行验证，不能提前宣布两个服务的重启问题完全解决。

### 4. APEX loop 挂载失败后的资源泄漏

runtime 和 VNDK 的 loop 设备读 sector 0 报 I/O 错误，之后使用 recovery loader 和已有库完成了本次解密。日志没有足够信息确定 I/O 错误的根因。

`79919eb` 修正可独立证明的清理缺口：read-only loop 增加 AUTOCLEAR，并持有设备句柄直到 mount 结束。mount 失败时释放绑定，成功时由 mount 持有直到卸载。不是对 APEX I/O 根因的修复，也没有改动内核或取消已有 fallback。

### 5. 诊断自身产生大量无关属性读取审计

原脚本枚举全部 Android 属性，产生了大量 getprop 的 permissive AVC，并把音频、相机等无关属性写入快照。`e79a9ae` 改为只查询 boot、版本、密钥服务和 USB 所需属性。保留内核、进程、挂载和 logcat；不改 SELinux 策略。

## 暂不改变的事项

- vendor 补丁属性：recovery 中已有 `2026-11-01`，采纳分区文件的 `2024-08-05` 被 init 拒绝。这次解密仍成功，不能再将该差异当成本次解密失败的已证实原因。未强改已成功环境的补丁值。
- USB：快照显示 adbd running、MTP/ADB FunctionFS ready、`sys.usb.state=mtp,adb`，但 UDC 是 not attached。未取得真实枚举/传输证据，不能把此前的 USB 现象都归结为用户态服务，也不能仅凭此判硬件损坏。
- SELinux：日志存在大量 recovery 域 permissive AVC，以及少量早期 init 域 enforcing AVC。当前成功依赖的策略边界需要另行审计；本轮没有切换 enforcing 或用宽泛规则消除告警。

## 候选镜像与验证边界

新候选：`~/skyward-work/out/skyward-kebab-20261107-followup.img`，100 MiB。

SHA-256：`1babc80c5ae448207b78c188f7440c6dcddd0b69564f8e0ebc624c0f29ceb498`。构建源码提交 `79919eb`。

- 72 项关联既有主机测试通过（block 32、config 24、pack 13、UI 状态栏 3）；Android arm64 release 构建通过。
- 主机首页预览已生成并检查。编写的是布局与打包功能，没有新增手机自动化测试。
- avbtool 验证 footer 和 recovery hash descriptor 通过；v2 DTBO 偏移已核对。
- kernel、second、DTBO、DTB 与成功镜像逐字节相同。ramdisk 仅变更 recovery binary、设备配置、诊断脚本和 Skyward rc，新增 logd 与四个缺失库。其余旧文件逐字节相同。
- 最终镜像内置 binary、配置和五个支持文件与构建输入逐字节相同。
- 本轮只取证和主机构建，没有刷写新候选或重启手机；设备保留当前已解密成功的版本。

构建清单：`~/skyward-work/out/skyward-kebab-20261107-followup.manifest.json`。主机预览命令：

```sh
target/debug/skyward-preview /tmp/kebab-preview --home --config devices/kebab.toml --lang en-US
```
