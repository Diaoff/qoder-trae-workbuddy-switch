/**
 * Loomy 微信扫码登录对话框（二期）。
 *
 * ## 为什么放在面板里而不是拉起浏览器
 *
 * 微信授权页**直接内嵌**二维码 uuid，长轮询端点用纯 HTTP 拿结果
 * （协议见 `crates/buddy-switch-core/src/modules/loomy/wechat.rs` 模块头）——
 * 因此面板只要「展示二维码图片 + 定时轮询状态帧」，容器 / 服务端形态同样可用。
 *
 * ## 状态机（**404/405 语义不能读反**，蓝本踩过）
 *
 * `waiting`（408，待扫码）→ `scanned`（404，已扫待确认）→
 * `confirmed`（405，code 在这一帧）→ 立即 `bind-auth`：
 * - `bound === true` ⇒ 讯飞侧已绑手机号，直接 `skip` 换 session，登录完成；
 * - `bound === false` ⇒ 需要绑定手机号，切到「短信验证码」子步骤。
 *
 * ⚠️ 轮询用 `setTimeout` 串行（不是 `setInterval`）：长轮询单帧本身可能挂
 * 25–40 秒，并发轮询会重复消费 code。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { Loader2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import * as api from "@/lib/api";

type LoomyScope = "loomy-cn";

/** 轮询间隔（毫秒）：长轮询单帧本身可能挂 25–40 秒，返回后稍作停顿再问下一帧。 */
const POLL_INTERVAL_MS = 1_000;
/** 二维码最长等待（毫秒）：超过即提示失效，请重新打开生成新码。 */
const POLL_DEADLINE_MS = 5 * 60 * 1000;

interface Props {
  scope: LoomyScope;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 登录成功（账号已落库）—— 调用方负责刷新账号列表。 */
  onLoggedIn: () => void;
}

type Phase =
  | { kind: "idle" }
  | { kind: "loading" }
  | {
      kind: "scanning";
      uuid: string;
      qrUrl: string;
      last: string;
      status: api.LoomyWechatPollStatus;
      startedAt: number;
    }
  | { kind: "bind-phone"; rcode: string; nickname: string | null; msgid: string | null }
  | { kind: "failed"; message: string };

/** 一句话解释当前轮询状态（scanning 子态）。 */
function scanningHint(status: api.LoomyWechatPollStatus): string {
  switch (status) {
    case "scanned":
      return "已扫码，请在手机上确认登录";
    default:
      return "请用微信扫描二维码";
  }
}

export function LoomyWechatLoginDialog({ scope, open, onOpenChange, onLoggedIn }: Props) {
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [phone, setPhone] = useState("");
  const [code, setCode] = useState("");
  const [submitting, setSubmitting] = useState(false);
  // 轮询循环的取消标记（关闭对话框 / 组件卸载时置位）。
  const cancelledRef = useRef(false);

  useEffect(() => {
    if (!open) {
      cancelledRef.current = true;
      setPhase({ kind: "idle" });
      setPhone("");
      setCode("");
    }
  }, [open]);

  /** 登录成功后的统一出口。 */
  const finish = useCallback(
    (uid: string) => {
      cancelledRef.current = true;
      setPhase({ kind: "idle" });
      onOpenChange(false);
      onLoggedIn();
      // uid 仅用于未来扩展（当前登录页不需要展示它）。
      void uid;
    },
    [onLoggedIn, onOpenChange],
  );

  /** 轮询循环：单帧返回后 setTimeout 再问下一帧（串行，绝不并发）。 */
  const pollLoop = useCallback(
    async (state: Extract<Phase, { kind: "scanning" }>) => {
      if (cancelledRef.current) return;
      try {
        const frame = await api.loomyWechatPoll(scope, state.uuid, state.last);
        if (cancelledRef.current) return;
        if (frame.status === "confirmed" && frame.code) {
          // ★ code 只用一次：立即换 rcode，然后按 bound 分支。
          const bind = await api.loomyWechatBindAuth(scope, frame.code);
          if (cancelledRef.current) return;
          if (bind.bound) {
            // 已绑手机号：直接 skip 换 session（拿不到手机号，uid 用 userid）。
            const result = await api.loomyWechatSkip(scope, bind.rcode, bind.nickname);
            finish(result.uid);
          } else {
            // 未绑手机号：进入短信验证子步骤（发码 → 校验 → 落库）。
            setPhase({
              kind: "bind-phone",
              rcode: bind.rcode,
              nickname: bind.nickname,
              msgid: null,
            });
          }
          return;
        }
        if (frame.status === "cancelled") {
          setPhase({ kind: "failed", message: "已取消登录（可在手机上重试或重新扫码）" });
          return;
        }
        if (frame.status === "expired") {
          setPhase({ kind: "failed", message: "二维码已失效，请重新打开登录框生成新码" });
          return;
        }
        // waiting / scanned：更新提示，继续轮询。超时兜底（二维码 5 分钟有效期）。
        if (Date.now() - state.startedAt > POLL_DEADLINE_MS) {
          setPhase({ kind: "failed", message: "等待超时（超过 5 分钟），请重新扫码" });
          return;
        }
        const next: Extract<Phase, { kind: "scanning" }> = {
          ...state,
          last: frame.status,
          status: frame.status,
        };
        setPhase(next);
        // setTimeout 而非 setInterval：等这一帧结束再安排下一帧。
        setTimeout(() => {
          if (!cancelledRef.current) void pollLoop(next);
        }, POLL_INTERVAL_MS);
      } catch {
        // 单帧彻底失败（理论少见，后端已兜网络错误）：短暂停顿后重试。
        if (!cancelledRef.current) {
          setTimeout(() => {
            if (!cancelledRef.current) void pollLoop(state);
          }, POLL_INTERVAL_MS * 3);
        }
      }
    },
    [scope, finish],
  );

  // 轮询循环不依赖 useEffect 重启：start() 拿到二维码后直接 kick off，
  // 后续帧由 setTimeout 链自己驱动（对 phase 重渲染不敏感）。

  /** 开始扫码：拉二维码并进入轮询循环。 */
  const start = useCallback(async () => {
    cancelledRef.current = false;
    setPhase({ kind: "loading" });
    try {
      const started = await api.loomyWechatStart(scope);
      const scanning: Extract<Phase, { kind: "scanning" }> = {
        kind: "scanning",
        uuid: started.uuid,
        qrUrl: started.qrUrl,
        last: "",
        status: "waiting",
        startedAt: Date.now(),
      };
      setPhase(scanning);
      // 立即开轮询链（不等 effect）。
      void pollLoop(scanning);
    } catch (err) {
      setPhase({ kind: "failed", message: api.asError(err) });
    }
  }, [scope, pollLoop]);


  /** 绑定手机号：发验证码。 */
  const sendBindCode = async (rcode: string) => {
    setSubmitting(true);
    try {
      const result = await api.loomyWechatBindSend(scope, rcode, phone);
      setPhase({ kind: "bind-phone", rcode, nickname: phase.kind === "bind-phone" ? phase.nickname : null, msgid: result.msgid });
    } catch (err) {
      setPhase({ kind: "failed", message: api.asError(err) });
    } finally {
      setSubmitting(false);
    }
  };

  /** 绑定手机号：校验验证码并落库。 */
  const checkBindCode = async (state: Extract<Phase, { kind: "bind-phone" }>) => {
    if (!state.msgid) return;
    setSubmitting(true);
    try {
      const result = await api.loomyWechatBindCheck(
        scope,
        state.rcode,
        code,
        state.msgid,
        state.nickname,
      );
      finish(result.uid);
    } catch (err) {
      setPhase({ kind: "failed", message: api.asError(err) });
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Loomy 微信扫码登录</DialogTitle>
          <DialogDescription>
            用微信扫码并确认；session 有效期 14 天，过期需重新登录（Loomy 无续期端点）。
          </DialogDescription>
        </DialogHeader>

        {phase.kind === "idle" || phase.kind === "loading" ? (
          <div className="flex flex-col items-center gap-3 py-6">
            {phase.kind === "loading" ? <Loader2 className="size-6 animate-spin" /> : null}
            <Button onClick={() => void start()} disabled={phase.kind === "loading"}>
              生成二维码
            </Button>
          </div>
        ) : null}

        {phase.kind === "scanning" ? (
          <div className="flex flex-col items-center gap-3 py-2">
            <img
              src={phase.qrUrl}
              alt="微信登录二维码"
              className="size-56 rounded-md border bg-white p-2"
            />
            <p className="flex items-center gap-2 text-sm text-muted-foreground">
              <Loader2 className="size-4 animate-spin" />
              {scanningHint(phase.status)}
            </p>
          </div>
        ) : null}

        {phase.kind === "bind-phone" ? (
          <div className="flex flex-col gap-3">
            <p className="text-sm text-muted-foreground">
              该微信尚未绑定手机号{phase.nickname ? `（${phase.nickname}）` : ""}，
              请输入手机号完成绑定。
            </p>
            <Input
              placeholder="11 位手机号"
              value={phone}
              inputMode="numeric"
              maxLength={11}
              onChange={(e) => setPhone(e.target.value.replace(/\D/g, ""))}
            />
            {phase.msgid ? (
              <Input
                placeholder="短信验证码"
                value={code}
                inputMode="numeric"
                onChange={(e) => setCode(e.target.value)}
              />
            ) : null}
          </div>
        ) : null}

        {phase.kind === "failed" ? (
          <p className="py-2 text-sm text-destructive">{phase.message}</p>
        ) : null}

        <DialogFooter>
          {phase.kind === "scanning" || phase.kind === "loading" ? (
            <Button variant="outline" onClick={() => onOpenChange(false)}>
              取消
            </Button>
          ) : null}
          {phase.kind === "bind-phone" && !phase.msgid ? (
            <Button
              onClick={() => void sendBindCode(phase.rcode)}
              disabled={submitting || phone.length !== 11}
            >
              {submitting ? <Loader2 className="size-4 animate-spin" /> : null}
              发送验证码
            </Button>
          ) : null}
          {phase.kind === "bind-phone" && phase.msgid ? (
            <Button
              onClick={() => void checkBindCode(phase)}
              disabled={submitting || code.length === 0}
            >
              {submitting ? <Loader2 className="size-4 animate-spin" /> : null}
              完成登录
            </Button>
          ) : null}
          {phase.kind === "failed" ? (
            <Button onClick={() => void start()}>重新扫码</Button>
          ) : null}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
