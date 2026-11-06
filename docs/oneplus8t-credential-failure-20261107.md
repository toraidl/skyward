# OnePlus 8T 设置锁屏凭据后的失败

用户澄清：此前成功时没有密码和指纹。新增密码与指纹后，用户文件不能打开。因此此前验收只覆盖无锁屏凭据路径，不能记为密码认证通过。

已确认 Wi-Fi ADB 目标为 KB2000 / 50a390b0，回到正常系统后，从 op2 取回对应 recovery 的持久快照。主机目录：`~/skyward-work/logs-20261107-credential/current/`，包括 recovery、kernel、logcat、状态和构建标识。

本次 recovery 日志显示：

```text
[23.278424] /dev/block/dm-15 holds f2fs at /data
[23.278481] the users whose own files are still closed: [0]
what was entered did not open the user's files: ask whether android.hardware.gatekeeper.IGatekeeper/default is described: the call failed in the driver with error -2147483647
```

这次 metadata 解密、F2FS 挂载成功，失败发生在用户凭据路径的服务发现阶段，先于本路径的 Gatekeeper verify 调用。日志不能解释为密码输入错误，也不能证明指纹是故障原因。

`Gatekeeper::open()` 原先对 AIDL 服务声明查询直接使用 `?`，使查询错误阻断后面的 HIDL Gatekeeper 1.0 发现。源码修正为：AIDL 明确声明时仍优先使用 AIDL；未声明或查询失败时查找 HIDL。仅在找到真实 HIDL handle 后回退；找不到时保留原查询错误。发现服务不提交凭据，不触发密码重试。

41 项现有 crypto 主机测试与 Android arm64 目标检查通过。尚未构建、刷入或验证此修正后的密码解锁；后续还需观察 HIDL verify、认证 token 与 CE key 解包。没有自动尝试密码或改动设备密钥文件。

附带确认：这次状态快照中 logd 与 gatekeeperd 已 running，logcat 文件约 293 KiB，说明上一轮日志程序修正已能提供进一步诊断；cryptoeng 仍为 restarting，需要另行取得其退出原因。
