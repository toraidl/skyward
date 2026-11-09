# OnePlus 8T：恢复模式振动

日期：2026-11-09。设备 KB2000 / 50a390b0。
提交 `e5d3b72`、`9bafe09`、`391e974`、`61aa19b`。实机确认有震动。

## 一、先走了错路：PMIC 的力反馈不是这台马达

`crates/skyward/src/haptics.rs` 原本只走内核力反馈（`FF_PERIODIC` +
`FF_CUSTOM`）。设备上唯一带 `FF_PERIODIC` 的输入设备是 `input4`，名字
`qti-haptics`，来自 PM8150B 自带的 haptics 模块。

现象是：效果上传返回 0、`EVIOCSFF` 返回 0、驱动日志也认了波形，**但手机不动**。
把 ioctl 成功当成马达动过，是这一轮最初的误判；这不是可用的验证。

真正的马达是 **AW8697**（i2c `6-005a`，`device_id=1815`，`f0` 实测 1700 左右），
接口是 LED class：`/sys/class/leds/vibrator`。`qti-haptics` 在本机不驱动任何东西。

## 二、AW8697 的播放路径

该内核的 AW8697 驱动带 `CONFIG_OPLUS_HAPTIC_OOS`，`activate` 不是走 RAM 循环，
而是按 `duration` 选一段 RTP 波形播放：

```c
if (duration <= 500) {
    val = (duration - 1) / 20 + rtp_max_num - AW8697_LONG_VIB_BIN_COUNT;
} else {
    val = rtp_max_num - 1;
}
```

也就是 40 ms 落 `40ms_RTP.bin`，2000 ms 落 `500ms_RTP.bin`。`gain` 上限 0x75。
波形文件在 `/odm/firmware/`（100 多个 `*_RTP.bin`）。

## 三、真正让它不动的原因：固件没加载

驱动在 6 秒探测时请求 `aw8697_haptic_170.bin`，而 `/odm` 是 Skyward 在 15.6 秒
才挂载的，于是：

```text
[15.369582] aw8697_ram_loaded: failed to read aw8697_haptic_170.bin
```

`ram_init` 为假之后，`activate_store` 只把 `state` 置 1 就返回，马达不动；
`duration` 读回来永远是 0（hrtimer 没启动），`state` 永远是 1。
设置里的“振动强度”滑杆确实在写 `gain`/`duration`（日志可见），但没人播。

修法：`Haptics::open` 在 LED class 分支里写一次 `ram_update`，让驱动在 `/odm`
挂好之后重新加载 RAM 镜像：

```text
[15.976728] (1)[647:recovery]aw8697_ram_update: haptic_real_f0 [170]
[15.985020] aw8697_ram_loaded: check sum pass : 0xc22b
[16.142544] aw8697_ram_loaded: fw update complete
```

之后一次点击（与 UI 同一路径）：

```text
[51.098535] aw8697_rtp_work_routine: rtp file [40ms_RTP.bin] size = 988
[51.098548] aw8697_rtp_work_routine: rtp file [40ms_RTP.bin] size = 988, f0 = 1700
```

实机确认有震动。

## 四、配置与交互

```toml
[haptics]
led = "/sys/class/leds/vibrator"
strength = { lightest = 0.25, strongest = 0.75 }
```

- `led` 存在时不打开力反馈设备；`click`/`resonance` 与它互斥，配置校验会拒绝。
- 点击时长 `BUZZ_MS = 40`，按 `gain = share × 128` 写强度（驱动再夹到 0x75）。
- 页面只在“点中并执行了某个控件”时置 `App::tap_worked`，由运行时播一次；
  点空白、滚动、长按后抬手都不重复播。长按、左滑返回、截图和弦仍走原来的
  `Effect::Haptic`。

## 五、仍未验收

- 40 ms + 低强度下是否每次都清晰可感（当前默认振动强度在 10% 附近）。
- 其他使用 PM8150B 力反馈的机型仍走原路径，未受影响。
