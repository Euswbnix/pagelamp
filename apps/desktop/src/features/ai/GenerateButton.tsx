import { notifyManager, useQueryClient } from "@tanstack/react-query";
import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import { flushSync } from "react-dom";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import {
  type AiBackendStatus,
  type AiStatus,
  type BlockReason,
  backendKey,
  type CostEstimate,
  type EstimateRequest,
  type ModelChoice,
  type ModelDisclosure,
} from "@/api/ai";
import {
  aiKeys,
  useAcknowledgeUnpricedModel,
  useAiStatus,
  useCodexStatus,
  useCostEstimate,
} from "@/api/ai-queries";
import { useApi } from "@/api/context";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { paths } from "@/lib/routes";
import { ModelDisclosureDialog, noteChanged } from "./DisclosureDialog";
import { estimateAmount, formatTokens } from "./lib/money";
import { useAiErrorText } from "./useAiErrorText";

/**
 * Whether the cost line applies. No model and the gate's blocks (the course's rules, question
 * (b)) carry no estimate; the others (disclosure, unpriced model, weekly cap, budget) leave it
 * complete, and over the budget the student decides on the override with it.
 */
function showsCost(block: BlockReason | null): boolean {
  return (
    block === null ||
    block === "disclosure_not_acknowledged" ||
    block === "budget_reached" ||
    block === "price_unknown_not_acknowledged" ||
    block === "weekly_run_cap_reached"
  );
}

/** Blocks the student resolves in Settings → AI models. */
const SETTINGS_BLOCKS = new Set(["no_model_chosen"]);

/**
 * Where the steps before Generate keep what outlives them: the note being read (its sheet stays
 * open while the estimate moves on, and a branch above may swap Generate out), and where focus
 * goes once a step is settled: to what comes next rather than to the page. A screen that can
 * swap Generate for something else wraps the swap in it, with a place for focus when Generate
 * is gone; a GenerateButton without one makes its own.
 */
export function GenerateSteps({
  children,
  fallbackFocus,
}: {
  children: ReactNode;
  /** Where focus goes when a step was settled and Generate is gone (e.g. the section heading). */
  fallbackFocus?: () => HTMLElement | null;
}) {
  const [note, setNote] = useState<OpenNote | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const fallback = useRef(fallbackFocus);
  useEffect(() => {
    fallback.current = fallbackFocus;
  });
  const steps = useMemo<Steps>(
    () => ({
      openNote: setNote,
      // The query client tells the page of new data in its next notification, after the call
      // that asked for it returned: this goes in the same queue, after it, so the page then
      // shows the estimate asked again (and whatever took Generate's place).
      moveOn: () =>
        notifyManager.schedule(() =>
          (nextStep(root.current) ?? fallback.current?.() ?? null)?.focus(),
        ),
    }),
    [],
  );
  return (
    <StepsContext.Provider value={steps}>
      {/* A box of its own, so the page's spacing still reaches it; none when it holds nothing. */}
      <div ref={root} className="empty:hidden">
        {children}
      </div>
      {note ? (
        <ModelDisclosureDialog
          backend={note.choice.backend}
          runsHere={note.runsHere}
          disclosure={note.disclosure}
          name={note.name}
          kind={note.kind}
          model={note.choice.model}
          changed={note.changed}
          open
          onOpenChange={(open) => {
            if (!open) setNote(null);
          }}
          // Radix returns focus a task after the sheet goes, by when the estimate asked again on
          // accepting it is shown.
          returnFocus={() => nextStep(root.current) ?? fallback.current?.() ?? null}
        />
      ) : null}
    </StepsContext.Provider>
  );
}

/** What GenerateSteps shares with the GenerateButton inside it. */
interface Steps {
  openNote: (note: OpenNote) => void;
  /** A step's call and the estimate asked again after it are done: focus moves on. */
  moveOn: () => void;
}

const StepsContext = createContext<Steps | null>(null);

/** The next thing to do after a settled step: the next step's control, or Generate. */
function nextStep(root: HTMLElement | null): HTMLElement | null {
  return (
    root?.querySelector<HTMLElement>("[data-next-step]") ??
    root?.querySelector<HTMLElement>("[data-generate]") ??
    null
  );
}

/**
 * "≈ $x" and Generate (design §3.5, §7): the facade's upper-bound estimate for this request,
 * refreshed as the request changes, and the pre-flight blocks it reports (`would_block`). The
 * blocks the student can settle here are settled here, one after the other: the note of the
 * model where it runs (a model that a server here may run in the cloud has one of its own),
 * then an unpriced model, acknowledged once; going over the budget is an explicit, per-run
 * choice. Everything else disables Generate with the reason. Nothing runs before each is done.
 */
export function GenerateButton(props: GenerateButtonProps) {
  const steps = useContext(StepsContext);
  const button = <Generate {...props} />;
  return steps ? button : <GenerateSteps>{button}</GenerateSteps>;
}

interface GenerateButtonProps {
  /** null = the form isn't complete yet. */
  request: EstimateRequest | null;
  onGenerate: (options: { overrideBudget: boolean }) => void;
  label?: string;
}

function Generate({ request, onGenerate, label }: GenerateButtonProps) {
  const { t, i18n } = useTranslation("ai");
  const estimate = useCostEstimate(request);
  const status = useAiStatus();
  const errorText = useAiErrorText();
  const steps = useContext(StepsContext) as Steps;
  const [overrideBudget, setOverrideBudget] = useState(false);
  const ids = { line: useId(), override: useId(), reason: useId() };

  const data = estimate.data ?? null;
  const block = data?.would_block ?? null;
  const blocked = block !== null && !(block === "budget_reached" && overrideBudget);
  const disabled = request === null || !data || blocked || estimate.isFetching;

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-3">
        <Button
          type="button"
          data-generate
          aria-disabled={disabled || undefined}
          aria-describedby={block ? `${ids.line} ${ids.reason}` : ids.line}
          className="aria-disabled:opacity-50"
          onClick={() => {
            if (!disabled) onGenerate({ overrideBudget: block === "budget_reached" });
          }}
        >
          {label ?? t("estimate.generate")}
        </Button>
        <p id={ids.line} className="text-sm text-muted-foreground" aria-live="polite">
          {estimate.isPending && request ? <Spinner aria-hidden /> : null}
          {data && request && showsCost(block) ? (
            <CostLine estimate={data} status={status.data ?? null} feature={request.feature} />
          ) : null}
        </p>
      </div>
      {data && request && data.input_tokens > 0 ? (
        <p className="text-xs text-muted-foreground">
          {t("estimate.details", {
            input: formatTokens(data.input_tokens, i18n.language),
            output: formatTokens(data.max_output_tokens + data.reasoning_allowance, i18n.language),
          })}
        </p>
      ) : null}
      {estimate.isError ? (
        <p role="alert" className="text-sm text-destructive">
          {errorText(estimate.error)}
        </p>
      ) : null}
      {block === "budget_reached" ? (
        <div className="space-y-1.5">
          <p id={ids.reason} className="text-sm font-medium">
            {t("estimate.overBudget")}
          </p>
          <div className="flex items-center gap-2">
            <Checkbox
              id={ids.override}
              checked={overrideBudget}
              onCheckedChange={(v) => setOverrideBudget(v === true)}
            />
            <Label htmlFor={ids.override} className="font-normal">
              {t("estimate.overrideBudget")}
            </Label>
          </div>
        </div>
      ) : null}
      {block === "disclosure_not_acknowledged" && request ? (
        <ModelDisclosureStep
          status={status.data ?? null}
          feature={request.feature}
          hintId={ids.reason}
          onOpen={steps.openNote}
          onSettled={steps.moveOn}
        />
      ) : null}
      {block === "price_unknown_not_acknowledged" && request ? (
        <UnpricedAcknowledgement
          status={status.data ?? null}
          feature={request.feature}
          hintId={ids.reason}
          onSettled={steps.moveOn}
        />
      ) : null}
      {block &&
      block !== "budget_reached" &&
      block !== "price_unknown_not_acknowledged" &&
      block !== "disclosure_not_acknowledged" ? (
        <p id={ids.reason} className="text-sm">
          {t(`blocked.${block}`)}{" "}
          {SETTINGS_BLOCKS.has(block) ? (
            <Link to={paths.settings} className="underline underline-offset-4">
              {t("settings.title")}
            </Link>
          ) : null}
        </p>
      ) : null}
    </div>
  );
}

/** A model's note, opened from its block. */
interface OpenNote {
  choice: ModelChoice;
  runsHere: boolean;
  disclosure: ModelDisclosure;
  name: string;
  kind: AiBackendStatus["kind"];
  /** This note changed since the student last accepted it. */
  changed: boolean;
}

function CostLine({
  estimate,
  status,
  feature,
}: {
  estimate: CostEstimate;
  status: AiStatus | null;
  feature: EstimateRequest["feature"];
}) {
  const { t, i18n } = useTranslation("ai");
  const upper = estimate.micro_usd_upper ?? null;
  if (upper === 0) return <>{t("estimate.free")}</>;
  if (upper !== null) {
    const shown = estimateAmount(upper, i18n.language);
    return (
      <>
        <span className="sr-only">{t("estimate.label")}</span>{" "}
        {shown.kind === "lessThan"
          ? t("estimate.lessThan", { amount: shown.amount })
          : t("estimate.upTo", { amount: shown.amount })}
      </>
    );
  }
  const choice = status?.features.find((f) => f.feature === feature)?.choice ?? null;
  const backend = choice
    ? status?.backends.find((b) => backendKey(b.backend) === backendKey(choice.backend))
    : undefined;
  if (backend?.kind === "codex") return <CodexCostLine />;
  if (backend?.kind === "local") return <>{t("estimate.cloudNoPrice")}</>;
  if (backend?.kind === "api_key") return <>{t("estimate.noPrice")}</>;
  return null;
}

/** Mode A has no price: the plan, and this week's runs against the cap. */
function CodexCostLine() {
  const { t } = useTranslation("ai");
  const codex = useCodexStatus();
  const cap = codex.data?.weekly_cap ?? null;
  if (codex.data && cap !== null) {
    return <>{t("codex.costLineRuns", { runs: codex.data.runs_this_week, cap })}</>;
  }
  return <>{t("codex.costLine")}</>;
}

/**
 * The note to read before this model runs, settled here: where the model runs is decided as the
 * run decides it (a server on this computer is asked about the model), and that place's note is
 * opened, so a model that the server may run in the cloud gets its own note, apart from the
 * server's. Once read, the estimate is asked again: the next block, if any, follows. Asking the
 * server can find the note already read (its list changed since the estimate): then only the
 * estimate is asked again, and if it still asks for a note, Settings (which has every note of
 * the backend) is where to read it. The ChatGPT plan, not set up yet, is set up in Settings first.
 */
function ModelDisclosureStep({
  status,
  feature,
  hintId,
  onOpen,
  onSettled,
}: {
  status: AiStatus | null;
  feature: EstimateRequest["feature"];
  hintId: string;
  onOpen: (note: OpenNote) => void;
  onSettled: () => void;
}) {
  const { t } = useTranslation("ai");
  const api = useApi();
  const client = useQueryClient();
  const errorText = useAiErrorText();
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<unknown>(null);
  // The note was found read, and the estimate asked again still wants one.
  const [inSettings, setInSettings] = useState(false);
  const reasonId = useId();
  const choice = status?.features.find((f) => f.feature === feature)?.choice ?? null;
  const backend = choice
    ? status?.backends.find((b) => backendKey(b.backend) === backendKey(choice.backend))
    : undefined;
  if (!choice || !backend) return null;
  if (inSettings || (backend.kind === "codex" && backend.state === "needs_setup")) {
    return (
      <p id={hintId} className="text-sm">
        <span id={reasonId}>{t("blocked.disclosure_not_acknowledged")}</span>{" "}
        <Link
          to={paths.settings}
          data-next-step
          aria-describedby={reasonId}
          className="underline underline-offset-4"
        >
          {t("settings.title")}
        </Link>
      </p>
    );
  }

  async function read() {
    if (!choice || !backend || checking) return;
    setChecking(true);
    setError(null);
    try {
      const runsHere = await api.modelRunsHere(choice, true);
      const disclosure = await api.modelDisclosure(choice.backend, runsHere);
      const [facts, acknowledged] = disclosure;
      if (acknowledged) {
        await client.invalidateQueries({ queryKey: aiKeys.all });
        // On the page before focus moves; shown only if this block is still there then.
        flushSync(() => setInSettings(true));
        onSettled();
        return;
      }
      onOpen({
        choice,
        runsHere,
        disclosure,
        name: backend.label,
        kind: backend.kind,
        changed: noteChanged(backend, facts),
      });
    } catch (err) {
      // E.g. the server here can't be reached: nothing is shown and nothing runs.
      setError(err);
    } finally {
      setChecking(false);
    }
  }

  return (
    <div className="space-y-1.5">
      <p id={hintId} className="text-sm">
        {t("blocked.disclosure_not_acknowledged")}
      </p>
      <Button
        type="button"
        size="sm"
        variant="outline"
        data-next-step
        aria-disabled={checking || undefined}
        className="aria-disabled:opacity-50"
        onClick={() => void read()}
      >
        {checking ? t("estimate.checkingWhere") : t("estimate.readDisclosure")}
      </Button>
      {error ? (
        <p role="alert" className="text-sm text-destructive">
          {errorText(error)}
        </p>
      ) : null}
    </div>
  );
}

function UnpricedAcknowledgement({
  status,
  feature,
  hintId,
  onSettled,
}: {
  status: AiStatus | null;
  feature: EstimateRequest["feature"];
  hintId: string;
  onSettled: () => void;
}) {
  const { t } = useTranslation("ai");
  const acknowledge = useAcknowledgeUnpricedModel();
  const errorText = useAiErrorText();
  const choice = status?.features.find((f) => f.feature === feature)?.choice ?? null;
  if (!choice) return null;

  async function use() {
    if (!choice || acknowledge.isPending) return;
    try {
      // Resolves once the estimate was asked again.
      await acknowledge.mutateAsync({ backend: choice.backend, model: choice.model });
      onSettled();
    } catch {
      // Shown below (acknowledge.error).
    }
  }
  return (
    <div className="space-y-1.5">
      <p id={hintId} className="text-sm">
        {t("estimate.useUnpricedHint")}
      </p>
      <Button
        type="button"
        size="sm"
        variant="outline"
        disabled={acknowledge.isPending}
        data-next-step
        onClick={() => void use()}
      >
        {t("estimate.useUnpriced")}
      </Button>
      {acknowledge.error ? (
        <p role="alert" className="text-sm text-destructive">
          {errorText(acknowledge.error)}
        </p>
      ) : null}
    </div>
  );
}
