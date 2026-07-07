import LocutionMark from "../icons/LocutionMark";
import LocutionTextLogo from "../icons/LocutionTextLogo";

const OnboardingLockup: React.FC = () => (
  <div className="flex flex-row items-center justify-center gap-2">
    <LocutionMark
      width={28}
      height={30}
      colorClassName="fill-logo-primary stroke-logo-primary"
      className="shrink-0"
    />
    {/* LocutionTextLogo's viewBox (0 0 640 140) is wider than the rendered
        glyph — the wordmark asset carries trailing empty space that's
        invisible when it centers alone, but visibly skews this icon+word
        lockup left of the true visual center. Trim the dead space out of
        layout with a measured negative margin (macOS-only target, so the
        rendered glyph width is deterministic — no cross-platform risk). */}
    <div className="-mr-[84px]">
      <LocutionTextLogo width={200} />
    </div>
  </div>
);

export default OnboardingLockup;
