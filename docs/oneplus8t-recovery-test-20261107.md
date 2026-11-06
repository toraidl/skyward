# OnePlus 8T recovery 解密试机记录

日期：2026-11-07。用户授权通过正常系统 Wi-Fi ADB 写 recovery，由用户手动重启。

## 已写入的镜像

- 设备：KB2000 / 50a390b0，ADB `192.168.144.228:5555`，当前槽 `_a`。
- 写入目标：`/dev/block/by-name/recovery_a` -> `/dev/block/sda16`。
- 镜像：`/home/bruce/skyward-work/out/skyward-kebab-20261107-decrypt-diag.img`，104857600 字节。
- 镜像和分区写回 SHA-256：`6c5323bb9bc0c83a024aeb94eed326f30110b00e2f30158e7524bc0a628ffa33`。
- 源码提交：`ee8efe9`，包含此前的两阶段 storage key 与 Binder 生命周期修正。
- 原厂底图：`~/skyward-work/stock-official/recovery.img`；kernel 沿用 `skyward-kebab-419325-loader.img` 中的完整 kernel section，stock DTBO/DTB 保留。
- 离线核对：AVB footer 与 recovery hash descriptor 通过 avbtool 校验；v2 DTBO 偏移正确；ramdisk 中 recovery binary 与诊断脚本逐字节匹配本次构建输入。
- 本地现有测试 122 项通过；aarch64 Android release 构建通过。
- 写入前确认 BCB command/recovery 参数为空；执行 dd、fsync 和 sync 后，整分区读回哈希一致。未发出重启指令，也未修改槽位。

## 原镜像备份

主机：`/home/bruce/skyward-work/backups/20261107/recovery_a-before.img`。

手机：`/data/local/tmp/skyward-recovery-a-backup-20261107.img`。

两份备份 SHA-256 均为 `b5a0f5980de3552cdb635280f858bb3c6fed78448175d264a0143f2ed9df250c`，与写入前原分区一致。

若需恢复，在确认仍为同一设备且回到正常系统后，可将这份手机备份 dd 回 recovery_a 并再次校验；不要将此命令当作当前试机步骤执行。

## 不依赖 data 解密的持久日志

该机实际持久日志文件系统为 `op2`（ext4，512 MiB）。`reserve2/3` 未识别出文件系统，本次没有向这些裸分区写入。

- 优先目录：`/mnt/vendor/oplusreserve/skyward/current/`。
- op2 无法挂载时的备用目录：`/mnt/vendor/persist/skyward/current/`。
- 前一次启动保存在各自的 `previous/`；只轮换 Skyward 专属目录。
- 文件：`build.txt`、`boot-id`、`status.txt`、`kernel.log`、`recovery.log`、`logcat.log`、`last-sync.txt`。
- 每约 3 秒更新，临时文件与目标在同一文件系统，以 rename 替换后 sync；各文件设大小上限，两次启动连同临时文件约占数 MiB。
- 诊断由 init 的独立服务启动，先于解密结果，不需要 USB 或 data 可用。kernel/init 尚未运行到此服务时，无法依靠此脚本留日志。

已在正常 Android 上运行同一脚本 10 秒验证：op2 下实际生成快照，时间戳更新，按预期 timeout 结束。另用原厂 ramdisk 的 toybox 验证 blkid/timeout 调用兼容性；脚本在主机和 Android shell 上语法检查通过。recovery 中的 SELinux、挂载及完整启动行为仍待本次试机确认，persist 为后备路径。

## 用户试机与后续取证

用户手动进入 recovery 后尝试解密。如果仍失败，在报错后停留至少 10 秒再回正常系统；恢复 Wi-Fi ADB 后，从上述优先目录及备用目录拉取日志即可，无需再次进入 recovery 或解密 data。

此时写入和读回校验已完成；recovery 启动、真实解密和跨重启日志留存尚不能记为通过。

构建清单：`~/skyward-work/out/skyward-kebab-20261107-decrypt-diag.manifest.json`。

写入回执：`~/skyward-work/backups/20261107/write-recovery-a.log`。
