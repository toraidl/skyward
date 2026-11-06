# OnePlus 8T Wi-Fi 与 OTA 适配实现

## 取证与修正

目标为 KB2000 / 50a390b0，正常系统通过 `192.168.144.228:5555` 取证。未操作另一个 USB 设备 PEDM00。新镜像尚未刷入，本轮没有重启设备、切槽或安装 OTA。

最新 recovery 日志显示密码操作后 closed users 从 `[0]` 变为 `[]`，已有密码解密成功证据。OTA 的独立失败原因如下：

- `update_engine.log` 报 payload minor version 9 超出原厂引擎支持的 `[1, 7]`，最终 `kUnsupportedMinorPayloadVersion (45)`。
- 同一日志有 metadata 签名验证失败，旧引擎继续执行后才因版本失败。本轮未添加签名跳过选项。
- 引擎在解析失败前已经把目标 B 槽标为不可启动；不能把“尚未写 payload”理解为没有改变任何槽状态。本轮未调整该状态。
- `~/ofox_12.1/system/update_engine` 中存在用户修改的一加 fake-full 适配，最大 minor version 为 9，并处理部分更新元数据及保留分区。本轮采用用户确认可刷包的实际编译产物，不自行只改版本常量。

日志副本：`~/skyward-work/ota-wifi-20261107/{current,exported}/`。当前自动日志没有引擎日志，手动导出的 `exported/update_engine.log` 提供了以上直接证据。新诊断脚本会把 `/tmp/update_engine.log` 的末尾 512 KiB 一并保存至同一持久目录。

## OTA 引擎

新增打包参数：

```
--update-engine-root /home/bruce/ofox_12.1/out/target/product/opkona/recovery/root
```

从该 recovery root 读取 `system/bin/update_engine_sideload`、`system/bin/linker64`，通过主机 `readelf` 递归解析 DT_NEEDED。缺失依赖、错误 ABI 或目标文件重名时停止打包。总计 29 个 ELF 放到 `/system/skyward-update-engine/`，引擎与 linker 放在目录顶层，库放在 `lib64/`。

原 `/system/bin/update_engine_sideload` 替换为启动脚本，清除 LD_PRELOAD、设置仅指向私有目录的 LD_LIBRARY_PATH，并直接运行匹配的 linker。Skyward 的 payload/offset/headers/status_fd 参数保持兼容。原厂 `/system/lib64` 文件不被覆盖，避免更换解密服务的公共依赖。

引擎源文件 SHA-256：`d6df899f5019a1119a421476f20cb5a85b35deea8b55f1a9b46cfd310970388e`。

版本显示改为优先读取非空 `version_name`，缺失时回退 `post-build-incremental`。已用真实包执行主机端 `skyward identify`，结果为 `an A/B update to KB2000_17.0.1.201(CN01)`，不再是 `1793346101331`。只改变显示，不改变设备、时间戳、补丁级别检查。

## Wi-Fi

- WLAN 配置支持 HIDL `hardware_services`，与原有 AIDL `service` 二选一。8T 的标准、QTI、Oplus supplicant 接口进入硬件服务目录描述。
- 使用当前 recovery 内核已有的 WLAN 驱动，不加载其他内核的模块。
- 开启 Wi-Fi 时先只读挂载当前槽 modem 的 vfat 文件系统，准备 QCA6390 的 amss20.bin、m3.bin、bdwlan.elf；从 vendor 和本机 persist 获取 INI、MAC 资产。完成后才写 CNSS fs_ready 并等待 wlan0。
- 直接解压最终保留的 kernel 验证请求字符串，INI 目标为 `wlan/WCNSS_qcom_cfg.ini`，MAC 目标为 `wlan/qca_cld/wlan_mac.bin`。与正常系统的带 qca6390 前缀路径不同。资产来源仍为本机 QCA6390 对应文件。
- 本机 vendor supplicant 及传递依赖共 32 个 ELF 已静态核对，无缺失依赖或强符号。原厂 ramdisk 缺少的六个接口库取自设备 VNDK v30，使用现有 `--library` 参数仅补缺失文件。
- supplicant 以错误级别输出，禁止启用 key/debug dump；stdout/stderr 分别最多保存 64 KiB，超出部分继续排空以避免阻塞进程。持久诊断同时保存这两个文件，不保存 supplicant 配置或用户输入密码。
- 联网探测地址为配置项 `http://connectivitycheck.gstatic.com/generate_204`。探测失败与关联/DHCP 成功是不同状态，受当前网络可达性影响，需实测。

接口库为 android.hardware.wifi.supplicant@1.0–1.3、android.hardware.wifi@1.0、android.system.wifi.keystore@1.0，副本在 `~/skyward-work/wifi-20261107/vndk-lib64/`。

## 镜像与验证

产物：`~/skyward-work/out/skyward-kebab-20261107-wifi-ota.img`

SHA-256：`4b752532fa5776c072da3c3f912867e9550be2a71ee91652b864b7c928b8bad5`

- 100 MiB，boot header v2；AVB NONE footer、vbmeta 和 recovery SHA-256 descriptor 验证通过。此项不表示厂商签名镜像。
- 独立解包最终 cpio，核对全部 35 个新支持资产 SHA-256，恢复程序和设备配置与本次源码产物相符。
- 与上次已部署的 gatekeeper 镜像相比，kernel、second、DTBO、DTB 不变，所有旧 system/lib64 文件及其权限/属主不变。已有文件只改变 recovery、引擎入口、设备配置、诊断脚本。
- 既有 config/install/pack 单元测试分别 24/66/13 项通过；Android arm64 release 编译通过；诊断脚本和引擎入口脚本语法检查通过。

构建参数、支持资产来源与哈希、依赖审计、最终镜像验证记录在 `~/skyward-work/ota-wifi-20261107/`，分别为 `build-arguments.json`、`support-manifest.json`、`engine-dependency-audit.json`、`image-verification.json`。Wi-Fi 审计为 `~/skyward-work/wifi-20261107/supplicant-dependency-audit-complete.json`。

## 待实机验收

下一轮需要依次验证：有密码解密仍成功；Wi-Fi 开关后 wlan0 出现；supplicant 控制 PONG；真实扫描列表；关联、DHCP、路由与 DNS；断网重连。静态链接通过不能证明这些行为已经成功。

OTA 需要在 recovery 中重新选择同一真实包，核对确认页版本和新引擎日志，并确认是否通过原失败点。完整刷包是分区写入操作，应与用户测试安排一起进行，不自动从正常系统启动引擎。日志应在重启后仍可从 op2 的 skyward/current 取得。

上次已验证密码解密的镜像仍为 `~/skyward-work/out/skyward-kebab-20261107-gatekeeper.img`（SHA-256 `6d7dc5684e239a71bc1f2f7667b708a7da665e0a951cab944a84b23afcadaa67`）；本轮没有覆盖设备上的恢复版本。
