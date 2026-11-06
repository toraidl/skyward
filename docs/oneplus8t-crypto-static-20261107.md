# OnePlus 8T 解密失败：静态复核与正常系统只读取证

日期：2026-11-07。源码起点 `6941a91`，本轮修正 `ae547b6`、`72f861c`。

## 范围与结论

本轮只通过 `192.168.144.228:5555` 查询正常 Android 和拉取程序库，在主机分析源码和 ELF。未调用设备密钥操作、运行 Skyward、停止服务、修改属性、重绑 USB、重启或刷写。另一台 USB 设备 PEDM00 未操作。

确认了两个源码缺陷：硬件包装密钥选错输入，以及 HIDL 回复的嵌套缓冲区提前释放。它们分别会阻断正确的解密流程、使返回数据读取不可靠。两项已在源码修正；目前不能把它们称为已经通过 recovery 实测的最终根因，也不能据此宣布解密可用。

## 1. 确认：wrappedkey_v0 分支把包装密钥当成存储密钥

旧的 `crates/skyward-crypto/src/data.rs::hardware_key()` 在 `wrapped=true` 时直接导出 `StoredKey::blob()`，即磁盘上的 `keymaster_key_blob`，跳过了 `encrypted_key`。

正确的数据关系是：

```text
keymaster_key_blob + secdiscardable 派生的 application ID
                     |
                     v
          AES-GCM 解开 encrypted_key（必须 finish 成功）
                     |
                     v
      不透明的长期 storage-key blob（仍然不是磁盘明文密钥）
                     |
                     v
       HIDL exportKey(RAW) / KeyMint convertStorageKeyToEphemeral
                     |
                     v
       本次启动的 ephemeral wrapped key -> dm-default-key / fscrypt
```

本地 vold 参考链：

- `~/ofox_12.1/system/vold/KeyStorage.cpp::retrieveKey()` 调用 `decryptWithKeymasterKey()` 取回存储密钥。
- `MetadataCrypt.cpp::read_key()` 使用这份结果；`create_crypto_blk_dev()` 再调用 `exportWrappedStorageKey()`。
- `KeyStorage.cpp::exportWrappedStorageKey()` 和 `Keymaster.cpp::exportKey()` 最终请求存储密钥转换。
- `system/security/keystore2/src/km_compat/km_compat.cpp::convertStorageKeyToEphemeral()` 去掉硬件 blob 的 `pKMblob` 前缀后调用 `exportKey(RAW, storageKeyBlob, {}, {})`。
- Skyward 自己的 CE 路径 `data.rs::open_user_files()` 也先解包，再转换，与旧的 metadata/DE 分支不一致。

设备本轮只读目录信息：metadata 的 `encrypted_key` 和 `keymaster_key_blob` 均为 228 字节，`secdiscardable` 为 16384 字节，version 为 1。二者相同长度不代表是同一个密钥对象。228 字节的 GCM 容器在 12 字节 nonce、16 字节 tag 的格式下容纳 200 字节 payload；长度计算本身不证明解包成功或 payload 有效。本轮没有读取或导出密钥内容。

提交 `ae547b6` 恢复先解包、后转换；覆盖共用此函数的 metadata、全局 DE 和用户 DE。此前 `1baef74` 关于“wrappedkey_v0 应直接导出 keymaster_key_blob”的判断有误，这次予以纠正。单独剥离前缀不能修正选错输入对象的问题。

## 2. 确认：Keymaster HIDL 返回值存在释放后读取

旧的 `keymaster.rs::call()`：

```rust
let reply = self.device.transact(code, &message)?;
Ok(reply.bytes().to_vec())
```

复制的只是顶层 HIDL 数据，其中的字符串、参数及输出向量仍是指向 Binder scatter/gather 缓冲区的地址。`skyward-binder/src/driver.rs::Reply::drop()` 发出 `BC_FREE_BUFFER`；局部 `reply` 在 `call()` 返回时就被释放，外层解析器随后仍通过这些地址取数据。

提交 `72f861c` 改为返回有所有权的 `BinderReply`，使其存活到调用方解析、复制完全部嵌套数据之后。hardware info、characteristics、begin/update/finish、export 和 upgrade 均使用这条通路。

这是源码可证明的生命周期错误；不能由此断定每一次读取都会立刻变坏，更不能把此前所有 `-33` 都归因于它。错误码本身是顶层标量；该缺陷主要损害嵌套输出的可靠性。

## 3. 不支持用 TEESimulator 替代当前 storage-key 转换

正常系统的 Keymaster 4.1 进程为 PID 837、keystore2 为 PID 872；本轮 maps 显示后者同时加载 `/system/lib64/libteesim_keymint.so` 和 OhMyKeymint 注入器。加载存在是事实，某次存储密钥调用被拦截则尚无证据。

本地 TEESimulator 源码有更明确的能力边界：

- `keymint/keymint_router.cpp:743` 的转换接口无条件转发给真实后端，缺后端则返回 `-100`。
- `rust/teesim-km/src/lib.rs:217` 配置 `sk_wrapper: None`，没有存储密钥包装后端。
- `third_party/keymint/ta/src/lib.rs::convert_storage_key_to_ephemeral()` 在缺少该后端时返回 Unimplemented。
- 模拟器自己生成的 blob 带 `TEESIMkm\0` 标记，不能把 `pKMblob` 或 QTI CBOR 中的 `KMKD` 标识当成模拟器来源证明。

设备拉取的 `libkeymint_fake_latest.so` 中，`AndroidKeyMintDevice::convertStorageKeyToEphemeral()` 位于 `0x1721c`、长度 48 字节，反汇编显示直接构造 `-100` 错误。它也没有在这个接口中实现替代转换。

因此不应基于“系统有模拟器”就把整套模拟器加入 recovery 解密链。正常 Android 的数据分区在启动期已成功打开，也不能证明启动完成后再调用任意 HAL 仍能复现同样的版本、密钥路由及 TEE 状态。

## 4. 已收窄的固件与版本问题

从本地原厂 `~/skyward-work/stock-official/recovery.img` 解析 v2 ramdisk，提取以下库并与正常系统 vendor 文件比较，三者 SHA-256 都一致：

| 文件 | SHA-256 |
| --- | --- |
| libqtikeymaster4.so | `9675031207009be5f27861cda07bdea3b921d9a4fa81d957262d73f9f2745269` |
| libkeymasterutils.so | `8d92872bfe7aead75eecece9206dae52ec2087d6e669ac7f7d2136831def78ba` |
| libkeymasterdeviceutils.so | `b4a8585f9cf832a8c3203b0c272f447ccee85fab947b82fd816219f7b87d641a` |

这削弱了“仅这三份库的版本不同”的解释；不证明 loader、依赖库、设备权限、服务依赖、属性或安全世界状态相同。

REA 对 QTI 库的反汇编还表明：

- `Keymaster41Device::exportKey()` 是向旧接口实现转发的包装。只把 4.1 service 换为 4.0 不能视为必然绕过问题。
- `KeymasterHalDevice::export_key()` 同时有 legacy 与另一条命令路径；legacy 内部仍调用 `spu_init()`。不能由 legacy 名称推断它完全不依赖相关初始化。
- `KeymasterSerialize::parseKeyBlob()` 检查 CBOR 中的 `0x4b4d4b44` 标识，标识不匹配可返回 `-33`。同一个错误也可能来自其他层；仅看到 `-33` 无法定位到 TEE 拒绝、前缀还是用户态解析。

仍存在版本输入差异：正常系统 `getprop ro.vendor.build.security_patch` 为 `2026-11-01`，而 `/vendor/build.prop` 为 `2024-08-05`。Skyward 从分区文件采纳属性，两种环境并不一致。尚未证明哪个值是密钥创建及 TEE 配置时的真实输入，不应随意改大日期。先前遗留的 bring_up 容错还可能在无法读到系统属性时继续启动服务，应在下一次 recovery 取证时记录服务启动前的实际属性。

## 5. 保留的客户端风险与下一次验收点

`keymaster.rs::update()` 读取后丢弃 `inputConsumed`，而 `gcm_decrypt()` 只 update 一次就 finish；本地 vold 使用 `updateCompletely()`。如果 HAL 部分消费输入，当前实现会遗漏尾部。错误路径还缺少统一 abort 清理。这是静态存在的健壮性缺口，本轮没有证据证明它在 228 字节容器上实际触发，暂不混入上述两个提交。

恢复 USB 救援能力以后，最有区分度的验证是：

1. 固定单一 Keymaster 实例、实际加载库及服务启动前的版本属性。
2. 分开记录外层 GCM 的 begin/updateConsumed/finish 状态；finish 成功前不认定解包成功。
3. 仅把解出的完整 storage-key blob 传给转换接口；核对前缀处理和 STORAGE_KEY 属性，日志仅记录状态、类型、长度，不输出密钥字节。
4. 转换通过后再验 dm-default-key、F2FS 挂载、DE、口令认证、CE 和内部存储的文件内容。

本轮未重新验证此前记录提及的“内层 200 字节 blob 仍报 -33”。两处修正之后是否还有独立的 HAL/TEE 拒绝原因仍未知。

## 6. USB：当前证据落在连接检测层

正常 Android 本轮采样：

```text
sys.usb.config=adb; sys.usb.state=adb; init.svc.adbd=running
gadget UDC=a600000.dwc3; usb_data_enabled=enabled
UDC state=not attached; current_speed=UNKNOWN
USB present=0; online=0; typec_mode=Nothing attached; cc_orientation=0
dumpsys usb: connected=false; configured=false; power_role=no-power; data_role=no-data
```

它表明采样时系统未检测到有线附着，不能解释为单纯 adbd 未运行。如果当时确实插着电脑，应优先沿线材/电脑端口、接口/尾插/排线、CC/VBUS 检测及 PMIC/Type-C 驱动路径定位；快照不能在硬件故障和底层软件状态之间二选一。未插线时这些值正常。

本地内核 `smb5-lib.c` 的 USB present 来自 PMIC 插入状态，`dwc3-msm.c` 通过 extcon 通知驱动连接流程。这比重设 adb 属性更接近当前证据指向的位置。本轮未强制切换角色、解绑 UDC 或触发复位。

## 7. 本地验证与交付边界

- 从提交后的 Git index 导出独立源码副本，避免把原有未提交实验代码当成提交验证。
- 副本执行现有 `cargo test --offline -p skyward-crypto -p skyward-binder --lib`：crypto 41、binder 20，共 61 项通过。没有编写或运行手机 UI 自动化测试。
- 副本使用现有 NDK 28 编译器执行 `cargo check --offline -p skyward-crypto --target aarch64-linux-android`：通过。首次未指定 NDK 的检查因找不到交叉 C 编译器失败，随后使用显式 CC/AR 路径完成检查，未改永久配置。
- 主机测试不执行 Android 专属 data/Keymaster 代码；目标检查验证其编译，不替代硬件行为、链接产物或 recovery 启动验收。
- `git diff --check` 通过。两项代码修改独立提交；原有 fstab、分区映射、main、Keymaster 实验改动及 tools 保留在工作区，未并入本轮提交。

取证文件位于 `~/skyward-work/static-20261107/`：选取的身份与 USB 快照 `device-observations.json`、拉取/提取的 ELF、REA 原始证据 `rea-evidence.json`、`host-tests.log` 和 `android-check.log`。本报告无需依赖旧报告对当前源码状态的描述。
