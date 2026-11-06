# OnePlus 8T OTA 停滞与默认 TCP ADB

## 本轮直接证据

目标为 KB2000 / 50a390b0，强制重启后正常系统 `sys.boot_completed=1`、当前槽 `_a`。通过 `192.168.144.228:5555` 取回上次 recovery 自动保存的日志，副本为 `~/skyward-work/ota-hang-20261107/current/`，没有停止正常系统服务或清理 OTA 状态。

日志同时确认密码解密成功（closed users 变为 `[]`），以及 Wi-Fi 取得 `192.168.144.207` 后连通互联网。该地址是上次 recovery 的租约，不等于正常 Android 的 `.228`，下次仍以实际地址为准。

`update_engine.log` 的关键路径：

1. 创建多个目标槽快照后，为 my_region_b 映射 COW image；随后打开 `/dev/block/dm-0` 返回 ENOENT，快照创建失败。
2. 引擎执行自己的 fallback，取消快照状态并准备直接映射，更新了目标槽逻辑分区元数据，输出 `Will overwrite existing partitions. Slot A may be unbootable until update finishes!`。本轮没有主动启用、替换或绕过该策略。
3. `PreparePartitionsForUpdate done` 后，`GenerateOperationsForPartitionsNotInPayload` 报 `Failed to load static a/b partitions`，然后 transfer terminated / Terminating。没有安装成功记录。
4. 持久进程快照里引擎 PID 2308 仍存在，日志停在上述错误路径。由此推测卡在失败后的终止处理，但本轮旧日志没有 wchan/stack/I/O 采样，不能确定具体阻塞调用。

用户在约启动 89 秒与 1000 秒保存过安装页截图，后者已取回 `install-stuck.png`。画面有填满的进度条、Step 1 的 100% 及覆盖系统提示，不能当作安装完成证据。

## 已修复的显示错误

提交 `6d1ebef`：用引擎状态管道的 `set_progress` 更新写入百分比，替代 FUSE 的最远读取位置。此前 ZIP 检查先读到尾部，使最远读取位置在真正写入前就达到 100%；同时原实现忽略了引擎的进度消息。校验/收尾仍由引擎日志与 Step 切换判定，百分比不是最终成功条件。

## 补齐 A/B 分区列表

引擎源码 `partition_update_generator_android.cc` 的 GetAbPartitionsOnDevice 读取 `ro.product.ab_ota_partitions`，缺失会直接拒绝生成 partial update 中未包含分区的操作。原厂 recovery 没有提供该属性；正常系统属性和 `/my_manifest/build.prop` 给出同一份 37 项列表。不能把 OrangeFox 示例里的七项列表直接当作这台移植系统的完整列表。

提交 `6d01868`：设备配置新增 `[partitions].ab_ota`，内容与本轮实际正常系统属性逐项一致。打包器在 prop.default 及存在时的 secure.prop 中写入该值，清除这些文件中同名旧值，保证引擎启动前就能读取。名称只允许字母、数字、连字符和下划线，非 A/B 设备不得配置该列表。本轮未修补引擎的版本、签名判断或强制跳过快照。

快照节点 ENOENT 是独立问题，目前没有证据表明上述属性修正也能解决它；后续必须在有实时 ADB 的 recovery 中单独验收。

## 默认 TCP 5555

提交 `babf1a3`：新增可选 `[adb].tcp_port`，默认关闭，仅 kebab 配置 5555。打包器把 `service.adb.tcp.port=5555` 放进初始属性文件，并通过 on boot 主动 start adbd，避免依赖损坏的 USB gadget 是否完成绑定。端口必须是 1–65535，0、负数、65536 的 CLI 配置加载检查均被拒绝。

这使用现有调试 recovery 的 root/免授权 adbd 设置，只改变 recovery 镜像，不改变正常 Android 的 ADB 设置。Wi-Fi 联网后还需实际验证端口监听、路由及 `adb connect`，仅配置正确不视作已建立连接。

提交 `8b9f84e`：把 WLAN 工作线程启动的 supplicant 纳入安装前的进程文件锁定。原流程只处理 [[service]]，遗漏了 supplicant 的 vendor ELF/依赖；现在在释放 vendor 挂载前也锁定这些文件。USB sideload 仍有自己的 adbd/USB 切换流程，本轮首先验收从内部存储安装 OTA 时的 TCP 诊断，不承诺 USB sideload 期间 TCP 会保持。

提交 `17c3fcd`：有界持久诊断增加 5555 的 `/proc/net/tcp{,6}` 行、OTA 引擎的 status/io/wchan/stack 和线程等待状态，以及 A/B 分区列表与端口属性。不会记录命令环境、内存或用户密码。原来的每个文件大小上限和两次启动保留机制不变。

## 验证边界

- config/install/pack 既有测试 24/66/13 项通过；Android arm64 release 编译通过。
- 配置中的 37 项 A/B 分区名与本次正常系统完全一致，记录为 config-preflight.json。
- 默认 5555、引擎继续写入、快照正确创建和完整 OTA 成功是不同验收项；下一次安装前先验证 ADB，不能再凭静止的百分比判断是否可以中断。
