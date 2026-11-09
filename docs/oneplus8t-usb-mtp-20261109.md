# OnePlus 8T：恢复模式 USB 的 MTP 与主机枚举

日期：2026-11-09。设备 KB2000 / 50a390b0，槽位 `_a`，Skyward 恢复模式。
本轮改动提交 `10364f1`、`fff55dc`、`a424dd8`，候选镜像
`~/skyward-work/out/skyward-kebab-20261109-mtp3.img`（SHA-256
`19df27b28ae595bc607e4540b32f78e0813d1e72f1e7ffcd29deda8fb62e370c`），
已写入 `recovery_a`、`recovery_b` 并回读校验。

## 一、主机只看到 ADB、没有 MTP

`sys.usb.state` 一直是 `mtp,adb`、`sys.usb.ffs.mtp.ready=1`，但主机 `lsusb`
只看到一个 `Vendor Specific / ADB Interface`。设备侧看到：

```text
/config/usb_gadget/g1/configs/b.1/f1 -> functions/ffs.adb
/config/usb_gadget/g1/configs/b.1/f2 -> functions/ffs.adb
```

两个链接都指向 adb。原因不是 MTP 服务没起来，而是初始化顺序：

1. `/init.recovery.qcom.rc` 在 `sys.usb.ffs.ready=1` 时把 `f1` 链到 `ffs.adb`
   并写 UDC；
2. 随后配置切到 `mtp,adb`，Skyward 想改 `f1 -> ffs.mtp`，但 **configfs 不会
   替换控制器还持有的链接**，init 的 `symlink` 也不先 unlink，于是直接失败
   （内核日志 `symlink() failed: File exists`）。

单独 `rm` 也不行：绑定状态下 `rm` 会留下一个空目录，紧接着的 `symlink` 报
`Operation not permitted`。可行顺序是先 `write UDC ""` 解绑，再删两个链接、
重建、写 UDC 绑回。

实测该顺序后主机立刻变成：

```text
bNumInterfaces 2
  bInterfaceClass 6   Imaging / Still Image Capture   iInterface "MTP"
  bInterfaceClass 255 Vendor Specific                 iInterface "ADB Interface"
```

libmtp 能打开设备并读到 Skyward 回的 `OnePlus 8T`。

## 二、不要在 init 规则里做这件事

第一版把上面的顺序写进 `skyward-mtp.rc`，结果 `dmesg` 里该规则被触发 59 次、
每 ~5 秒一次：解绑会让 adbd 重新写 `sys.usb.ffs.ready`，而规则的触发条件包含
这个属性，于是自己触发自己。控制器被反复重建，主机枚举直接失败：

```text
usb 5-1: Device not responding to setup address.
usb 5-1: device not accepting address 75, error -71
```

这不是线的问题。同一根线在规则循环开始前（11:32:41）能正常枚举成
`18d1:4ee2`。曾把 -71 误判为 Type-C 线材问题，实际是这一轮引入的规则循环。

因此链接改由 Skyward 自己在 `skyward_sys::usb::set` 里做一次：解绑、清链接、
按 `ffs.mtp`/`ffs.adb` 重建、绑回、读回校验，然后自己写 `sys.usb.state`。
`skyward-mtp.rc` 里不再有任何写 UDC / symlink 的规则。

少了这条规则后，`sys.usb.state` 没有别的规则会写，`set()` 会一直等不到
`mtp,adb` 并报 `the port did not take file transfer`（屏幕上的一闪而过的
错误框）。`a424dd8` 让 `set()` 在链接完成后自己写该属性。

## 三、当前状态

```text
sys.usb.state        = mtp,adb
udc state            = configured
configs/b.1/f1       -> functions/ffs.mtp
configs/b.1/f2       -> functions/ffs.adb
主机 bNumInterfaces  = 2（MTP + ADB）
adb devices          = 50a390b0 recovery usb:5-1
内核日志             = 无 "did not take file transfer"，无规则循环
```

USB 上的 adb 与 MTP 同时可用。

## 四、仍未验收

- **存储列表**：`GetStorageIDs` 在锁屏未解锁时返回空，libmtp 因此报
  `Get Storage information failed`。这是既有设计——`Transfer::offer` 只在
  `app.state.data.storage_open()` 为真且没有擦除/安装任务时把 `/sdcard`
  交出去；设了密码的这台机器停在解密页时是 `Data::Locked { user: 0 }`，
  所以没有存储可列。需要在设备上输入密码解锁后再验一次真实文件传输。
- 未做主机侧真实拷入/拷出文件与哈希比对。
- 未验证 fastbootd 下 MTP/adb 的切换。
