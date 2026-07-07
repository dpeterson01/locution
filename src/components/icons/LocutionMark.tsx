const LocutionMark = ({
  width,
  height,
  className,
  colorClassName = "fill-text stroke-text",
}: {
  width?: number | string;
  height?: number | string;
  className?: string;
  colorClassName?: string;
}) => (
  <svg
    width={width || 126}
    height={height || 135}
    viewBox="0 0 126 135"
    className={className ? `${colorClassName} ${className}` : colorClassName}
    xmlns="http://www.w3.org/2000/svg"
  >
    <rect x="4" y="47" width="18" height="41" rx="9" />
    <rect x="30" y="30" width="18" height="75" rx="9" />
    <rect x="56" y="4" width="18" height="127" rx="9" />
    <rect x="82" y="30" width="18" height="75" rx="9" />
    <rect x="108" y="47" width="18" height="41" rx="9" />
  </svg>
);

export default LocutionMark;
