import { CoreConnectionInfo } from 'data/LodestoneContext';
import { isEdge } from 'react-device-detect';
import { LabelColor } from 'components/Atoms/Label';
import axios, { AxiosError, AxiosRequestConfig } from 'axios';
import { ClientError } from 'bindings/ClientError';
import { InstanceState } from 'bindings/InstanceState';
import { ClientFile } from 'bindings/ClientFile';
import { QueryClient } from '@tanstack/react-query';
import { Base64 } from 'js-base64';
import React from 'react';
import { LoginReply } from 'bindings/LoginReply';
import { LoginValues } from 'pages/login/UserLogin';
import { toast } from 'react-toastify';
import { UserPermission } from 'bindings/UserPermission';
import clsx, { ClassValue } from 'clsx';
// eslint-disable-next-line no-restricted-imports
import { extendTailwindMerge, twMerge } from 'tailwind-merge';

export const DISABLE_AUTOFILL = isEdge
  ? 'off-random-string-edge-stop-ignoring-autofill-off'
  : 'off';
export const LODESTONE_PORT = 16662;
export const DEFAULT_LOCAL_CORE: CoreConnectionInfo = {
  address: 'localhost',
  port: LODESTONE_PORT.toString(),
  protocol: 'http',
  apiVersion: 'v1',
};
export const myTwMerge = extendTailwindMerge({
  classGroups: {
    'font-size': [
      {
        text: [
          'caption',
          'small',
          'medium',
          'h3',
          'h2',
          'h1',
          'title',
          '2xlarge',
          '3xlarge',
          '4xlarge',
          '5xlarge',
          '6xlarge',
        ],
      },
    ],
  },
});

export const supportedZip = ["zip", "gz", "tgz"];

export const capitalizeFirstLetter = (string: string) => {
  return string.charAt(0).toUpperCase() + string.slice(1);
};

export function round(num: number, precision: number) {
  const factor = Math.pow(10, precision);
  return Math.round(num * factor) / factor;
}

/**
 * Call an async function with a maximum time limit (in milliseconds) for the timeout
 * @param {Promise<any>} asyncPromise An asynchronous promise to resolve
 * @param {number} timeLimit Time limit to attempt function in milliseconds
 * @returns {Promise<any> | undefined} Resolved promise for async function call, or an error if time limit reached
 */
export const asyncCallWithTimeout = async (
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  asyncPromise: Promise<any>,
  timeLimit: number
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
): Promise<any> => {
  let timeoutHandle: NodeJS.Timeout;

  const timeoutPromise = new Promise((_resolve, reject) => {
    timeoutHandle = setTimeout(
      () => reject(new Error('Async call timeout limit reached')),
      timeLimit
    );
  });

  return Promise.race([asyncPromise, timeoutPromise]).then((result) => {
    clearTimeout(timeoutHandle);
    return result;
  });
};

// a map from InstanceStatus to string names
// instancestatus is a union type
export const stateToLabelColor: { [key in InstanceState]: LabelColor } = {
  Running: 'green',
  Starting: 'yellow',
  Stopping: 'yellow',
  Stopped: 'gray',
  Error: 'red',
  // Loading: 'gray',
};

export const stateToColor: { [key in InstanceState]: string } = {
  Starting: "text-yellow-300",
  Running: "text-green-300",
  Stopping: "text-yellow-300",
  Stopped: "text-gray-faded/30",
  Error: "text-red-200",
};

export function isAxiosError<ResponseType>(
  error: unknown
): error is AxiosError<ResponseType> {
  return axios.isAxiosError(error);
}

export function errorToString(error: unknown): string {
  console.log(error)
  if (isAxiosError<unknown>(error)) {
    if (error.response && error.response.data) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      let data: any = error.response.data;

      /* if response.data is a string parse it as a JSON object */
      if (typeof data === 'string') {
        // spaghetti code
        if (data.startsWith('`Authorization`')) {
          return 'Invalid token: try to log out and log in again';
        }
        data = JSON.parse(data);
      }

      /* if response.data is a blob parse it as a JSON object */
      if (data instanceof Blob) {
        const reader = new FileReader();
        reader.readAsText(data);
        // reader.onload = () => {
        //   data = JSON.parse(reader.result as string);
        // };
      }

      if (data && data.kind) {
        // TODO: more runtime type checking
        const clientError: ClientError = new ClientError(data);
        return clientError.toString();
      } else return `${error.code}: ${error.response.statusText}`;
    } else {
      if (error.code === 'ERR_NETWORK') return 'Network error';
      else return `Network error: ${error.message}`;
    }
  }
  if (error === null) return '';
  if (error instanceof Error) return error.message;
  // if it's string
  if (typeof error === 'string') return error;
  return `Unknown error`;
}

/**
 * @throws Error with neatly formatted error message
 * @returns ResponseType
 */
export async function axiosWrapper<ResponseType>(
  config: AxiosRequestConfig
): Promise<ResponseType> {
  try {
    const response = await axios.request<ResponseType>(config);
    return response.data;
  } catch (error) {
    throw new Error(errorToString(error));
  }
}

export async function axiosPutSingleValue<ResponseType>(
  url: string,
  value: unknown
): Promise<ResponseType> {
  return axiosWrapper<ResponseType>({
    method: 'put',
    url,
    data: JSON.stringify(value),
    headers: {
      'Content-Type': 'application/json',
    },
  });
}

// meant to be used with an async function
// that throws an error if the condition is not met
// and returns void otherwise
// returns a string if the condition is not met
// returns empty string otherwise
export async function catchAsyncToString(
  promise: Promise<unknown>
): Promise<string> {
  try {
    await promise;
    return '';
  } catch (e) {
    if (e instanceof Error) return e.message;
    return 'Unknown error';
  }
}

export function parseintStrict(value: string): number {
  // parseint passes even if there are trailing characters
  // so we check that the string is the same as the parsed int
  const parsed = parseInt(value);
  if (parsed.toString() !== value) throw new Error('Not an integer');
  return parsed;
}

export function parseFloatStrict(value: string): number {
  // parseFloat passes even if there are trailing characters
  // so we check that the string is the same as the parsed float
  const parsed = parseFloat(value);
  if (parsed.toString() !== value) throw new Error('Not a valid float');
  return parsed;
}



export function getWidth(
  el: HTMLElement,
  type: 'inner' | 'outer' | 'width' | 'full'
) {
  if (type === 'inner')
    // .innerWidth()
    return el.clientWidth;
  else if (type === 'outer')
    // .outerWidth()
    return el.offsetWidth;
  const s = window.getComputedStyle(el, null);
  if (type === 'width')
    // .width()
    return (
      el.clientWidth -
      parseInt(s.getPropertyValue('padding-left')) -
      parseInt(s.getPropertyValue('padding-right'))
    );
  else if (type === 'full')
    // .outerWidth(includeMargins = true)
    return (
      el.offsetWidth +
      parseInt(s.getPropertyValue('margin-left')) +
      parseInt(s.getPropertyValue('margin-right'))
    );
  throw new Error('Invalid type');
}

export function getHeight(
  el: HTMLElement,
  type: 'inner' | 'outer' | 'height' | 'full'
) {
  if (type === 'inner')
    // .innerHeight()
    return el.clientHeight;
  else if (type === 'outer')
    // .outerHeight()
    return el.offsetHeight;
  const s = window.getComputedStyle(el, null);
  if (type === 'height')
    // .height()
    return (
      el.clientHeight -
      parseInt(s.getPropertyValue('padding-top')) -
      parseInt(s.getPropertyValue('padding-bottom'))
    );
  else if (type === 'full')
    // .outerHeight(includeMargins = true)
    return (
      el.offsetHeight +
      parseInt(s.getPropertyValue('margin-top')) +
      parseInt(s.getPropertyValue('margin-bottom'))
    );
  throw new Error('Invalid type');
}

// format duration in seconds to DD:HH:MM:SS
export const formatDuration = (duration: number) => {
  const days = Math.floor(duration / 86400);
  const hours = Math.floor((duration % 86400) / 3600);
  const minutes = Math.floor((duration % 3600) / 60);
  const seconds = Math.floor(duration % 60);
  return `${days < 10 ? '0' + days : days}:${hours < 10 ? '0' + hours : hours
    }:${minutes < 10 ? '0' + minutes : minutes}:${seconds < 10 ? '0' + seconds : seconds
    }`;
};

// format message time for notifications
export const formatNotificationTime = (time_ms: number) => {
  const now = new Date();
  const time = new Date(time_ms);
  if (
    now.getFullYear() === time.getFullYear() &&
    now.getMonth() === time.getMonth() &&
    now.getDate() === time.getDate()
  ) {
    return time.toLocaleTimeString('en-US');
  } else {
    return time.toLocaleString('en-US');
  }
};

// format "time ago"
export const formatTimeAgo = (time_ms: number) => {
  const now = new Date();
  const time = new Date(time_ms);
  const diff = now.getTime() - time.getTime();
  const diffDays = Math.floor(diff / (1000 * 3600 * 24));
  const diffHours = Math.floor(diff / (1000 * 3600));
  const diffMinutes = Math.floor(diff / (1000 * 60));
  const diffSeconds = Math.floor(diff / 1000);
  if (diffDays > 0) {
    return `${diffDays} day${diffDays > 1 ? 's' : ''} ago`;
  } else if (diffHours > 0) {
    return `${diffHours} hour${diffHours > 1 ? 's' : ''} ago`;
  } else if (diffMinutes > 0) {
    return `${diffMinutes} minute${diffMinutes > 1 ? 's' : ''} ago`;
  } else if (diffSeconds > 0) {
    return `${diffSeconds} second${diffSeconds > 1 ? 's' : ''} ago`;
  } else {
    return 'just now';
  }
};

// format "progress" number of bytes to human readable string
// using unit appropriate for "total"
export const formatBytesDownload = (
  progress: number,
  total: number,
  decimals = 2
) => {
  if (total === 0) return '0 Bytes';
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB', 'PB', 'EB', 'ZB', 'YB'];
  const i = Math.floor(Math.log(total) / Math.log(k));
  if (i > sizes.length - 1)
    throw new Error('The file is bigger than 1024 YB!? this should not happen');
  return parseFloat((progress / Math.pow(k, i)).toFixed(dm)) + ' ' + sizes[i];
};

// format number of bytes to human readable string
export const formatBytes = (bytes: number, decimals = 2) => {
  return formatBytesDownload(bytes, bytes, decimals);
};

export const LODESTONE_EPOCH = BigInt('1667530800000');

// get the timestamp from a snowflake (bitint)
export const getSnowflakeTimestamp = (snowflake: string) => {
  const snowflakeBigInt = BigInt(snowflake);
  return Number(snowflakeBigInt >> BigInt(22)) + Number(LODESTONE_EPOCH);
};


export const chooseFiles = async (multiple = true) => {
  const input = document.createElement('input');
  input.type = 'file';
  input.multiple = multiple;
  input.click();
  return new Promise<FileList | null>((resolve) => {
    input.onchange = () => {
      resolve(input.files);
    };
    input.oncancel = () => {
      resolve(null);
    };
  });
};

// put this in whatever utility library file you have:
export function convertUnicode(input: string) {
  return input
    .replace(/\\+u([0-9a-fA-F]{4})/g, (a, b) =>
      String.fromCharCode(parseInt(b, 16))
    )
    .replace(/\\+n/g, '\n');
}

// use combined refs to merge multiple refs into one
// could take React.RefObject<T> or React.ForwardedRef<T>
export function useCombinedRefs<T>(...refs: any[]) {
  const targetRef = React.useRef<T>(null);

  React.useEffect(() => {
    refs.forEach((ref: any) => {
      if (!ref) return;

      if (typeof ref === 'function') {
        ref(targetRef.current);
      } else {
        ref.current = targetRef.current;
      }
    });
  }, [refs]);

  return targetRef;
}

export const parentPath = (path: string, direcotrySeparator: string) => {
  const pathParts = path.split(direcotrySeparator);
  pathParts.pop();
  return pathParts.join(direcotrySeparator) || '.';
};

export const getFileName = (path: string, direcotrySeparator: string) => {
  const pathParts = path.split(direcotrySeparator);
  return pathParts[pathParts.length - 1];
};

// check if a core is localhost
export function isLocalCore(core: CoreConnectionInfo) {
  return (
    core.address === 'localhost' ||
    core.address === '127.0.0.1' ||
    core.address === '::1' ||
    /^0+:0+:0+:0+:0+:0+:0+:1+$/.test(core.address)
  );
}

export function cn(...inputs: ClassValue[]) {
  return myTwMerge(clsx(...inputs));
}

// detect if we are in a browser that supports negative lookbehind
export const negativeLookbehindSupported = (() => {
  try {
    new RegExp('(?<!\\w)foo');
    return true;
  } catch (e) {
    return false;
  }
})();

export const fileSorter = (a: ClientFile, b: ClientFile) => {
  if (a.file_type === b.file_type) {
    return a.name.localeCompare(b.name);
  }
  return a.file_type.localeCompare(b.file_type);
};
