/* eslint-disable i18next/no-literal-string -- static brand wordmark, never translated */
const LocutionTextLogo = ({
  width,
  height,
  className,
}: {
  width?: number;
  height?: number;
  className?: string;
}) => {
  return (
    <svg
      width={width}
      height={height}
      className={className}
      viewBox="0 0 640 140"
      xmlns="http://www.w3.org/2000/svg"
    >
      <text
        x="0"
        y="102"
        fontFamily="-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif"
        fontSize="96"
        fontWeight="700"
        letterSpacing="-2"
        className="logo-stroke"
      >
        Locution
      </text>
    </svg>
  );
};

export default LocutionTextLogo;
