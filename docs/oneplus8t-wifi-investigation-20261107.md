# OnePlus 8T recovery Wi-Fi 调查与适配路径

## 本轮边界

在用户授权刷入密码认证修正前，通过正常系统 `192.168.144.228:5555` 只读取证，确认序列号 50a390b0。未停止 supplicant、断开 Wi-Fi、扫描网络、加载模块、修改 MAC 或读取保存的网络密码。取证副本在 `~/skyward-work/wifi-20261107/`。

密码认证修正版已经写入 recovery_a 并读回校验，随后执行 `adb reboot recovery`。镜像 `~/skyward-work/out/skyward-kebab-20261107-gatekeeper.img`，SHA-256 `6d7dc5684e239a71bc1f2f7667b708a7da665e0a951cab944a84b23afcadaa67`。相对上一版只有 ramdisk 内的 recovery 可执行文件改变，未夹带 Wi-Fi 配置实验。备份及回执在 `~/skyward-work/backups/20261107-gatekeeper/`。

## 已证实的设备输入

- 芯片：正常系统内核日志明确输出 QCA6390 / HIF_TYPE_QCA6390，驱动为 cnss_pci，接口 wlan0，另有 p2p0。
- `/dev/wlan`、`/sys/module/wlan` 和写入型节点 `/sys/kernel/cnss/fs_ready` 存在；正常系统 lsmod 没有显示需要另行加载的 wlan 模块。历史 recovery 日志也已到 `wlan_hdd_state ... initialized`。这说明有驱动基础，不证明 recovery 已完成芯片启动和创建 wlan0。
- 正常系统内核日志读取 `qca6390/bdwlan.elf` 成功，显示固件 build `101604f8`。此前有 `qca6390/regdb.bin` 查找失败，但正常 Wi-Fi 仍运行，因此不能把该条单独认定为阻断原因。
- vendor INI：`/vendor/etc/wifi/qca6390/WCNSS_qcom_cfg.ini`。
- MAC 资产路径：`/mnt/vendor/persist/qca6390/wlan_mac.bin`，120 字节；本轮只检查文件存在和权限，没有读取内容。
- vendor 固件链接：`/vendor/firmware/wlan/qca_cld/qca6390/WCNSS_qcom_cfg.ini` 和同目录 `wlan_mac.bin` 分别指向上述 INI 与 persist 文件。
- supplicant 程序：`/vendor/bin/hw/wpa_supplicant`，本轮已取回 ELF 供离线检查。
- vendor rc 明确声明标准 HIDL supplicant 1.0–1.3、QTI 扩展 2.0–2.2、Oplus 扩展 2.0。lshal 声明与其一致，但部分条目没有 PID，不把声明当作主动调用成功。
- 正常系统有 supplicant、wificond、`android.hardware.wifi@1.0-service` 进程。recovery 不必复制整套 Android Wi-Fi 框架。

## 现有源码缺口

1. `devices/kebab.toml` 尚无 `[wlan]`，因此当前镜像没有提供本机 WLAN 启动资料。
2. `skyward-config::Wlan.service` 是必填字符串，现有设备示例使用 AIDL `android.hardware.wifi.supplicant.ISupplicant/default`。
3. `skyward/src/main.rs` 把 `wlan.service` 加入普通 Binder 服务目录描述；本机 supplicant 使用 HIDL，应该进入硬件服务描述路径。只换一个字符串不足以适配。
4. WLAN 的实际控制在 `skyward/src/wlan.rs` 和 `skyward-net/src/supplicant.rs` 中使用 Unix control socket，发送 PING、SCAN、ADD_NETWORK 等命令，已有 DHCP、DNS 和连通性状态逻辑。因此无需先重写完整的 HIDL Wi-Fi 控制客户端。
5. supplicant 的 DT_NEEDED 包含 `libnl.so`、`libkeystore-wifi-hidl.so`、`libkeystore-engine-wifi-hidl.so`、`libcert_parse.wpa_s.so`、标准 HIDL 1.0–1.3、QTI 2.0–2.2 和 Oplus 2.0 接口库。现有 recovery 的 VNDK APEX 挂载失败，必须检查这些库及其传递依赖、符号版本和 loader 组合，不能凭文件名存在认定可运行。

## 建议实现顺序

### A. 补 HIDL 登记模式

让 WLAN 配置明确区分普通 Binder `service` 与 `hardware_service`，且每项只选择一种。保留原有 AIDL 机型行为；8T 的标准 HIDL 接口及必要扩展加入 hwservicemanager 的描述准备流程，不送进 AIDL 的 publish_descriptions。

supplicant 仍由现有 WLAN 工作线程管理，使用自己的 `/tmp` 控制目录和配置，等待 control socket 后用 PING/PONG 验证就绪。不能因为 HIDL 名称登记了就提前宣布 Wi-Fi 可用。

### B. 分开验证驱动与固件

基于当前 recovery 内核核对内建 WLAN 能力；有内建驱动时不凭另一内核树编译 wlan.ko 填入。按 recovery 实际固件请求补 QCA6390 的 INI、MAC 与 BDF/固件可见路径，在必要固件准备好之后才写 cnss fs_ready。MAC 使用本机 persist 资产，不硬编码或生成替代地址。

保持固件来源分区只读；先验收到 wlan0 出现及可置 UP，不启动或修改正常系统的无线服务。

### C. 验证 supplicant 与扫描

先完成 ELF 依赖审计，再在 recovery 启动单一 supplicant；保留其 stderr/logcat，分别判断动态链接、HIDL 注册、nl80211、控制 socket、权限和 firmware 错误。现有启动代码把 stdout/stderr 丢到 null，建议为 WLAN 启动失败保留有界诊断输出。

验收顺序：socket PONG → SCAN → 返回真实 BSS 列表。不要从服务 running 或 wlan0 存在直接推断可以扫描。

### D. 连接与网络验收

用户在设备输入 Wi-Fi 密码，日志不保存密码。先验收关联成功，再 DHCP 地址、路由、DNS、HTTP 连通性；连通性探测地址用适合本目标的配置，不直接套用 Xiaomi 示例。最后再考虑显式开关 TCP ADB，并验证断网重连和多次开关 Wi-Fi。

在恢复 Wi-Fi 救援能力前，保留 op2 持久日志和可恢复的 recovery 备份；不能把正常系统当前 Wi-Fi ADB 可用当成 recovery Wi-Fi 已支持。

## 本轮结论

已确定芯片、固件资产入口、supplicant 接口类型与源码配置缺口。Wi-Fi 尚未修改、刷入或运行验收。下一步最小实验是 HIDL 登记适配及 QCA6390 驱动启动，逐层取得可区分的日志。
