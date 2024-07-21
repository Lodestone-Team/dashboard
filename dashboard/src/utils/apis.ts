import { QueryClient } from '@tanstack/react-query';
import axios from 'axios';
import { ClientError } from 'bindings/ClientError';
import { ClientFile } from 'bindings/ClientFile';
import { MacroEntry } from 'bindings/MacroEntry';
import { LoginReply } from 'bindings/LoginReply';
import { UserPermission } from 'bindings/UserPermission';
import { Base64 } from 'js-base64';
import { LoginValues } from 'pages/login/UserLogin';
import { toast } from 'react-toastify';
import {
  axiosWrapper,
  catchAsyncToString,
  errorToString,
  getFileName,
  isAxiosError,
  parentPath,
} from './util';
import { UnzipOption } from 'bindings/UnzipOptions';
import { CopyInstanceFileRequest } from 'bindings/CopyInstanceFileRequest';
import { ZipRequest } from 'bindings/ZipRequest';
import { TaskEntry } from 'bindings/TaskEntry';
import { HistoryEntry } from 'bindings/HistoryEntry';
import { GetConfigResponse } from 'bindings/GetConfigResponse';
import { SettingManifest } from 'bindings/SettingManifest';
import { PlayitSignupData } from 'bindings/PlayitSignupData';
import { PlayitTunnelInfo } from 'bindings/PlayitTunnelInfo';

/***********************
 * Start Files API
 ***********************/

export const getTmpPath = async () => {
  const tmpPath = await axiosWrapper<string>({
    method: 'get',
    url: `/fs/tmp_path`,
  });

  return tmpPath;
};

export const uploadFile = async (
  directory: string,
  file: Array<File>,
  overwrite = false,
) => {
  // upload all files using multipart form data
  const formData = new FormData();
  file.forEach((f) => {
    formData.append('file', f);
  });
  toast.info(`Uploading ${file.length} ${file.length > 1 ? 'files' : 'file'}`);
  const error = await catchAsyncToString(
    axiosWrapper<null>({
      method: 'put',
      url: `/fs/${Base64.encode(directory, true)}/upload` + (overwrite ? "/overwrite" : ""),
      data: formData,
      headers: {
        'Content-Type': 'multipart/form-data',
      },
      timeout: 0,
    })
  );
  if (error) {
    toast.error(error);
    return;
  }
};


export const saveInstanceFile = async (
  uuid: string,
  directory: string,
  file: ClientFile,
  content: string,
  queryClient: QueryClient
) => {
  const error = await catchAsyncToString(
    axiosWrapper<null>({
      method: 'put',
      url: `/instance/${uuid}/fs/${Base64.encode(file.path, true)}/write`,
      data: content,
    })
  );
  if (error) {
    toast.error(error);
    return;
  }
  queryClient.setQueryData(
    ['instance', uuid, 'fileContent', file.path],
    content
  );

  const fileListKey = ['instance', uuid, 'fileList', directory];
  const fileList = queryClient.getQueryData<ClientFile[]>(fileListKey);
  if (!fileList) return;
  const newFileList = fileList.map((f) => {
    if (f.path === file.path)
      return {
        ...f,
        modification_time: Math.round(Date.now() / 1000),
        size: content.length,
      };
    return f;
  });
  queryClient.setQueryData(fileListKey, newFileList);
};

export const deleteInstanceFile = async (
  uuid: string,
  directory: string,
  file: ClientFile,
  queryClient: QueryClient
) => {
  const error = await catchAsyncToString(
    axiosWrapper<null>({
      method: 'delete',
      url: `/instance/${uuid}/fs/${Base64.encode(file.path, true)}/rm`,
    })
  );
  if (error) {
    toast.error(error);
    return;
  } else {
    toast.success(`Deleted file: ${file.name}`);
  }

  const fileListKey = ['instance', uuid, 'fileList', directory];
  const fileList = queryClient.getQueryData<ClientFile[]>(fileListKey);
  if (!fileList) return;
  queryClient.setQueryData(
    fileListKey,
    fileList?.filter((f) => f.path !== file.path)
  );
};

export const deleteInstanceDirectory = async (
  uuid: string,
  parentDirectory: string,
  directory: string,
  queryClient: QueryClient
) => {
  const error = await catchAsyncToString(
    axiosWrapper<null>({
      method: 'delete',
      url: `/instance/${uuid}/fs/${Base64.encode(directory, true)}/rmdir`,
    })
  );
  if (error) {
    toast.error(error);
    return;
  } else {
    toast.success(`Deleted directory: ${directory}`);
  }
  const fileListKey = ['instance', uuid, 'fileList', parentDirectory];
  const fileList = queryClient.getQueryData<ClientFile[]>(fileListKey);
  queryClient.setQueryData(
    fileListKey,
    fileList?.filter((file) => file.path !== directory)
  );
};

export const requestInstanceFileUrl = async (
  uuid: string,
  file: ClientFile,
  timeout = 5000,
): Promise<string> => {
  const tokenResponse = await axiosWrapper<string>({
    method: 'get',
    url: `/instance/${uuid}/fs/${Base64.encode(file.path, true)}/url`,
    timeout: timeout,
  });
  return axios.defaults.baseURL + `/file/${tokenResponse}`;
};

export const downloadInstanceFile = async (uuid: string, file: ClientFile, timeout = 5000) => {
  // TODO handle errors

  window.open(await requestInstanceFileUrl(uuid, file, timeout), '_blank');
};

export const uploadInstanceFiles = async (
  uuid: string,
  directory: string,
  file: Array<File>,
  queryClient: QueryClient
) => {
  // upload all files using multipart form data
  const formData = new FormData();
  file.forEach((f) => {
    formData.append('file', f);
  });
  toast.info(`Uploading ${file.length} ${file.length > 1 ? 'files' : 'file'}`);
  const error = await catchAsyncToString(
    axiosWrapper<null>({
      method: 'put',
      url: `/instance/${uuid}/fs/${Base64.encode(directory, true)}/upload`,
      data: formData,
      headers: {
        'Content-Type': 'multipart/form-data',
      },
      timeout: 0,
    })
  );
  if (error) {
    toast.error(error);
    return;
  }

  // invalidate the query instead of updating it because file name might be different
  queryClient.invalidateQueries(['instance', uuid, 'fileList', directory]);
};

export const createInstanceFile = async (
  uuid: string,
  directory: string,
  name: string
) => {
  const filePath = directory + '/' + name;
  return await catchAsyncToString(
    axiosWrapper<null>({
      method: 'put',
      url: `/instance/${uuid}/fs/${Base64.encode(filePath, true)}/new`,
    })
  );
};

export const createInstanceDirectory = async (
  uuid: string,
  parentDirectory: string,
  name: string
) => {
  const filePath = parentDirectory + '/' + name;
  return await catchAsyncToString(
    axiosWrapper<null>({
      method: 'put',
      url: `/instance/${uuid}/fs/${Base64.encode(filePath, true)}/mkdir`,
    })
  );
};

export const copyRecursive = async (
  uuid: string,
  request: CopyInstanceFileRequest,
  direcotrySeparator: string,
  queryClient: QueryClient
) => {
  const error = await catchAsyncToString(
    axiosWrapper<null>({
      method: 'put',
      url: `/instance/${uuid}/fs/cpr`,
      data: request,
    })
  );
  if (error) {
    toast.error(error);
    return;
  }
  queryClient.invalidateQueries([
    'instance',
    uuid,
    'fileList',
    parentPath(request.relative_path_dest, direcotrySeparator),
  ]);
};

export const moveInstanceFileOrDirectory = async (
  uuid: string,
  source: string,
  destination: string,
  queryClient: QueryClient,
  direcotrySeparator: string
) => {
  const error = await catchAsyncToString(
    axiosWrapper<null>({
      method: 'put',
      url: `/instance/${uuid}/fs/${Base64.encode(
        source,
        true
      )}/move/${Base64.encode(destination, true)}`,
    })
  );

  if (error) {
    toast.error(
      `Failed to move ${getFileName(source, direcotrySeparator)}: ${error}`
    );
    return error;
  }

  // just invalided the query instead of updating it because file name might be different due to conflict
  queryClient.invalidateQueries([
    'instance',
    uuid,
    'fileList',
    parentPath(source, direcotrySeparator),
  ]);
  queryClient.invalidateQueries([
    'instance',
    uuid,
    'fileList',
    parentPath(destination, direcotrySeparator),
  ]);
};

export const zipInstanceFiles = async (
  uuid: string,
  zipRequest: ZipRequest
) => {
  const error = await catchAsyncToString(
    axiosWrapper<null>({
      method: 'put',
      url: `/instance/${uuid}/fs/zip`,
      data: zipRequest,
      headers: {
        'Content-Type': 'application/json',
      },
    })
  );
  if (error) {
    toast.error(`Failed to initiate zip: ${error}`);
    return;
  } else {
    toast.info(
      `Zipping ${zipRequest.target_relative_paths.length} item${zipRequest.target_relative_paths.length > 1 ? 's' : ''
      }...`
    );
  }
};

export const unzipInstanceFile = async (
  uuid: string,
  file: ClientFile,
  unzipOption: UnzipOption
) => {
  const error = await catchAsyncToString(
    axiosWrapper<null>({
      method: 'put',
      url: `/instance/${uuid}/fs/${Base64.encode(file.path, true)}/unzip`,
      data: unzipOption,
      headers: {
        'Content-Type': 'application/json',
      },
    })
  );

  if (error) {
    toast.error(`Failed to unzip ${file.name}: ${error}`);
    return;
  } else {
    toast.info(`Unzipping ${file.name}...`);
  }
};

/***********************
 * End Files API
 ***********************/

/***********************
 * Start User API
 ***********************/

/**
 * @throws string
 */
export async function loginToCore(
  loginValue: LoginValues
): Promise<LoginReply | undefined> {
  // we manually handle error here because we want to show different error messages
  try {
    return await axios
      .post<LoginReply>(
        '/user/login',
        {},
        {
          auth: {
            username: loginValue.username,
            password: loginValue.password,
          },
        }
      )
      .then((response) => {
        return response.data;
      });
  } catch (error) {
    if (isAxiosError<ClientError>(error) && error.response) {
      if (
        error.response.status === 401 ||
        error.response.status === 403 ||
        error.response.status === 500
      ) {
        throw `Error: ${error.response.data.kind}: ${error.response.data.causes}`;
      }
    } else {
      throw `Login failed: ${errorToString(error)}`;
    }
  }
}

/**
 * @throws string if error
 * @returns LoginReply if success
 */
export const createNewUser = async (values: {
  username: string;
  password: string;
}) => {
  return await axiosWrapper<LoginReply>({
    method: 'post',
    url: '/user',
    data: values,
  });
};

/**
 * @throws string if error
 * @returns undefined if success
 */
export const changePassword = async (values: {
  uid: string;
  old_password: string | null;
  new_password: string;
}) => {
  return await axiosWrapper<undefined>({
    method: 'put',
    url: `/user/${values.uid}/password`,
    data: values,
  });
};

/**
 * @throws string if error
 * @returns undefined if success
 */
export const deleteUser = async (uid: string) => {
  return await axiosWrapper<undefined>({
    method: 'delete',
    url: `/user/${uid}`,
  });
};

/**
 * @throws string if error
 * @returns undefined if success
 * @param uid user id
 */
export const changeUserPermissions = async (
  uid: string,
  permission: UserPermission
) => {
  return await axiosWrapper<undefined>({
    method: 'put',
    url: `/user/${uid}/update_perm`,
    data: permission,
  });
};

/***********************
 * End User API
 ***********************/

export const openPort = async (port: number) => {
  return await catchAsyncToString(
    axiosWrapper<null>({
      method: 'put',
      url: `/gateway/open_port/${port}`,
    })
  );
};

/***********************
 * Start Tasks/Macro API
 ***********************/

export const getTasks = async (uuid: string) => {
  const taskList = await axiosWrapper<TaskEntry[]>({
    method: 'get',
    url: `/instance/${uuid}/task/list`,
  });

  return taskList;
};

export const getMacros = async (uuid: string) => {
  const macroList = await axiosWrapper<MacroEntry[]>({
    method: 'get',
    url: `/instance/${uuid}/macro/list`,
  });

  return macroList;
};

// get macro config
export const getMacroConfig = async (uuid: string, macro_name: string) => {
  const macroConfig = await axiosWrapper<GetConfigResponse>({
    method: 'get',
    url: `/instance/${uuid}/macro/config/get/${macro_name}`
  })
  return macroConfig
}

// write macro local config
export const storeMacroConfig = async (
  uuid: string,
  macro_name: string,
  config: Record<string, SettingManifest>) => {
  const serverResponse = await axiosWrapper<unknown>({
    method: 'post',
    url: `/instance/${uuid}/macro/config/store/${macro_name}`,
    data: config
  })
  return serverResponse;
}

export const getInstanceHistory = async (uuid: string) => {
  const historyList = await axiosWrapper<HistoryEntry[]>({
    method: 'get',
    url: `/instance/${uuid}/history/list`,
  });

  return historyList;
};

//run macro
export const createTask = async (
  queryClient: QueryClient,
  uuid: string,
  macro_name: string,
  args: string[]
) => {
  queryClient.invalidateQueries(['instance', uuid, 'taskList']);
  return await catchAsyncToString(
    axiosWrapper<null>({
      method: 'put',
      url: `/instance/${uuid}/macro/run/${macro_name}`,
      data: args,
    })
  );
};

//run macro
export const killTask = async (
  queryClient: QueryClient,
  uuid: string,
  pid: string
) => {
  return await catchAsyncToString(
    axiosWrapper<null>({
      method: 'put',
      url: `/instance/${uuid}/macro/kill/${pid}`,
    })
  );
};
/***********************
 * End Tasks/Macro API
 ***********************/

/***********************
 * Playitgg API
 ***********************/
export const generatePlayitSignupLink = async (): Promise<PlayitSignupData> => {
  const response = await axiosWrapper<PlayitSignupData>({
    method: 'get',
    url: `/playitgg/generate_signup_link`,
  });
  return response;
};

export const verifyKey = async (): Promise<boolean> => {
  const response = await axiosWrapper<boolean>({
    method: 'post',
    url: `/playitgg/verify_key`,
  });
  return response;
};

export const startCli = async () => {
  return await catchAsyncToString(
    axiosWrapper<null>({
      method: 'post',
      url: `/playitgg/start_cli`,
    })
  );
};

export const stopCli = async () => {
  return await catchAsyncToString(
    axiosWrapper<null>({
      method: 'post',
      url: `/playitgg/stop_cli`,
    })
  );
};

export const cliIsRunning = async () => {
  const response = await axiosWrapper<boolean>({
    method: 'get',
    url: `/playitgg/cli_is_running`,
  });
  return response;
};

export const getTunnels = async () => {
  const response = await axiosWrapper<PlayitTunnelInfo[]>({
    method: 'get',
    url: `/playitgg/get_tunnels`,
  });
  return response;
};
