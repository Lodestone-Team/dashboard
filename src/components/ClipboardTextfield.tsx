import { faClone } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { useRef, useState } from 'react';
import Tooltip from 'rc-tooltip';

export default function ClipboardTextfield({
  text,
  textToCopy,
  className,
}: {
  text: string;
  textToCopy?: string;
  className?: string;
}) {
  const onClickCopy = (e: React.MouseEvent<HTMLDivElement>) => {
    e.stopPropagation();
    navigator.clipboard.writeText(textToCopy || text);
    //TODO: toast "copied" when we have notifications setup
    alert(`Copied "${textToCopy || text}"`);
  };

  return (
    <Tooltip
      overlay={<span>Copy to clipboard</span>}
      placement="top"
      trigger={['hover']}
      mouseEnterDelay={0.2}
    >
      <div
        className={`hover:cursor-pointer select-none group ${className}`}
        onClick={onClickCopy}
      >
        {' '}
        {/* TODO develop custom tooltip component */}
        {text}&nbsp;&nbsp;
        <FontAwesomeIcon
          className={`text-gray-faded/30 group-hover:text-gray-500`}
          icon={faClone}
        />
      </div>
    </Tooltip>
  );
}
