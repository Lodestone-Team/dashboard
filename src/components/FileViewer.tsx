import Editor, { useMonaco } from '@monaco-editor/react';
import * as monaco from 'monaco-editor/esm/vs/editor/editor.api';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import {
  faFolder,
  faFile,
  faClipboardQuestion,
  faFloppyDisk,
  faDownload,
  faTrashCan,
  faUpload,
  faFilePen,
  faFolderPlus,
  faAngleDown,
  faPlus,
  faCheckSquare,
} from '@fortawesome/free-solid-svg-icons';
import { Fragment, useContext, useEffect, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { ClientFile } from 'bindings/ClientFile';
import { InstanceContext } from 'data/InstanceContext';
import axios from 'axios';
import { FileType } from 'bindings/FileType';
import {
  axiosWrapper,
  catchAsyncToString,
  chooseFiles,
  createInstanceDirectory,
  createInstanceFile,
  deleteInstanceDirectory,
  deleteInstanceFile,
  downloadInstanceFiles,
  formatTimeAgo,
  saveInstanceFile,
  uploadInstanceFiles,
} from 'utils/util';
import Button from 'components/Atoms/Button';
import { useLocalStorage } from 'usehooks-ts';
import InputField from 'components/Atoms/Form/InputField';
import { Form, Formik } from 'formik';
import ResizePanel from 'components/Atoms/ResizePanel';
import { Dialog, Menu, Transition } from '@headlessui/react';
import { faSquare } from '@fortawesome/free-regular-svg-icons';
import clsx from 'clsx';
import * as yup from 'yup';

type Monaco = typeof monaco;

const fileSorter = (a: ClientFile, b: ClientFile) => {
  if (a.file_type === b.file_type) {
    return a.name.localeCompare(b.name);
  }
  return a.file_type.localeCompare(b.file_type);
};

const useFileList = (uuid: string, path: string) =>
  useQuery<ClientFile[], Error>(
    ['instance', uuid, 'fileList', path],
    () => {
      return axiosWrapper<ClientFile[]>({
        url: `/instance/${uuid}/fs/ls/${path}`,
        method: 'GET',
      }).then((response) => {
        // sort by file type, then file name
        return response.sort(fileSorter);
      });
    },
    {
      retry: false,
      cacheTime: 0,
      staleTime: 0,
    }
  );

const useFileContent = (uuid: string, file: ClientFile | null) =>
  useQuery<string, Error>(
    ['instance', uuid, 'fileContent', file?.path],
    () => {
      return axiosWrapper<string>({
        url: `/instance/${uuid}/fs/read/${file?.path}`,
        method: 'GET',
        transformResponse: (data) => data,
      }).then((response) => {
        return response;
      });
    },
    {
      enabled: file !== null,
      cacheTime: 0,
      staleTime: 0,
      retry: false,
    }
  );

export default function FileViewer() {
  const { selectedInstance: instance } = useContext(InstanceContext);
  const monaco = useMonaco();
  const queryClient = useQueryClient();
  const editorRef = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  if (!instance) throw new Error('No instance selected');
  const [path, setPath] = useState('');
  const [openedFile, setOpenedFile] = useState<ClientFile | null>(null);
  const [fileContent, setfileContent] = useState('');
  const fileContentRef = useRef<string>();
  fileContentRef.current = fileContent;
  const [createFileModalOpen, setCreateFileModalOpen] = useState(false);
  const [createFolderModalOpen, setCreateFolderModalOpen] = useState(false);
  const [fileListSize, setFileListSize] = useLocalStorage('fileListSize', 200);
  const [tickedFiles, setTickedFiles] = useState<ClientFile[]>([]);
  const tickFile = (file: ClientFile, ticked: boolean) => {
    if (ticked) {
      setTickedFiles((files) => [...files, file]);
    } else {
      setTickedFiles((files) => files.filter((f) => f.path !== file.path));
    }
  };
  const fileTicked = (file: ClientFile) => {
    // check just the path and type, not other metadata
    return tickedFiles.some(
      (f) => f.path === file.path && f.file_type === file.file_type
    );
  };

  const atTopLevel = path === '';
  let direcotrySeparator = '\\';
  // assume only linux paths contain /
  if (instance.path.includes('/')) direcotrySeparator = '/';

  const parentPath = (path: string) => {
    const pathParts = path.split(direcotrySeparator);
    pathParts.pop();
    return pathParts.join(direcotrySeparator);
  };

  /* Query */

  const {
    data: fileList,
    isLoading: fileListLoading,
    error: fileListError,
  } = useFileList(instance.uuid, path);

  const {
    data: originalFileContent,
    isLoading: isFileLoading,
    error: fileError,
  } = useFileContent(instance.uuid, openedFile);

  useEffect(() => {
    setfileContent(originalFileContent || '');
  }, [originalFileContent]);

  useEffect(() => {
    setfileContent('');
  }, [openedFile]);

  /* Monaco */

  function handleEditorDidMount(
    editor: monaco.editor.IStandaloneCodeEditor,
    monaco: Monaco
  ) {
    editorRef.current = editor;
    // add ctrl+s save
    if (!instance) return;
    if (!openedFile) return;
    editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () =>
      saveInstanceFile(
        instance.uuid,
        path,
        openedFile,
        fileContentRef.current || '',
        queryClient
      )
    );
  }

  // hack to get .lodestone_config detected as json
  const monacoPath =
    openedFile?.name === '.lodestone_config'
      ? '.lodestone_config.json'
      : openedFile?.name;

  const showingMonaco = openedFile && !isFileLoading && !fileError;

  useEffect(() => {
    // set monaco theme, just a different background color
    if (monaco) {
      monaco.editor.defineTheme('lodestone-dark', {
        base: 'vs-dark',
        inherit: true,
        rules: [],
        colors: {
          'editor.background': '#26282C',
          'editor.lineHighlightBackground': '#2c2e33',
        },
      });
    }
  }, [monaco]);

  /* Helper functions */

  const chooseFilesToUpload = async () => {
    const files = await chooseFiles();
    if (!files) return;
    // convert FileList to Array
    const fileArray = Array.from(files);
    await uploadInstanceFiles(instance.uuid, path, fileArray, queryClient);
  };

  const deleteTickedFiles = async () => {
    // TODO: show a confirmation dialog
    if (!tickedFiles) return;
    for (const file of tickedFiles) {
      if (file.file_type === 'Directory') {
        deleteInstanceDirectory(instance.uuid, path, file.path, queryClient);
        tickFile(file, false);
      } else if (file.file_type === 'File') {
        deleteInstanceFile(instance.uuid, path, file, queryClient);
        tickFile(file, false);
      }
    }
    setTickedFiles([]);
  };

  const downloadTickedFiles = async () => {
    if (!tickedFiles) return;
    const missedDirectories: string[] = [];
    for (const file of tickedFiles) {
      if (file.file_type === 'Directory') {
        missedDirectories.push(file.path);
      } else if (file.file_type === 'File') {
        downloadInstanceFiles(instance.uuid, file);
        tickFile(file, false);
      }
    }
    if (missedDirectories.length > 0) {
      const missedDirectoriesString = missedDirectories.join(', ');
      // TODO: make this a toast
      alert(
        `Downloading a directory is not supported. The following directories were not downloaded: ${missedDirectoriesString}`
      );
    }
  };

  /* UI */

  const breadcrumb = (
    <div className="flex min-w-0 grow select-none flex-row flex-nowrap items-start gap-1 whitespace-nowrap text-base font-medium">
      <p className="truncate">
        {/* instance name */}
        <span
          className={
            path !== '' || openedFile
              ? 'cursor-pointer text-blue-accent hover:underline'
              : 'text-gray-300'
          }
          onClick={() => {
            setPath('');
            setOpenedFile(null);
            setTickedFiles([]);
          }}
        >
          {instance.path.split(direcotrySeparator).pop()}
        </span>

        {/* path */}
        {path &&
          path.split(direcotrySeparator).map((p, i, arr) => {
            // display a breadcrumb, where each one when clicked goes to appropriate path
            const subPath = arr.slice(0, i + 1).join(direcotrySeparator);
            return (
              <span key={subPath}>
                <span className="text-gray-300"> {direcotrySeparator} </span>
                <span
                  className={
                    i !== arr.length - 1 || openedFile
                      ? 'cursor-pointer text-blue-accent hover:underline'
                      : 'text-gray-300'
                  }
                  onClick={() => {
                    setPath(subPath);
                    setOpenedFile(null);
                    setTickedFiles([]);
                  }}
                >
                  {p}
                </span>
              </span>
            );
          })}
      </p>

      {/* file name */}
      <p className="truncate text-gray-300">
        <span className="text-gray-300"> {direcotrySeparator} </span>
        {openedFile?.name}
      </p>
    </div>
  );

  const fileTreeEntryClassName =
    'flex flex-row items-center gap-4 py-2 px-4 text-base font-medium whitespace-nowrap';

  const fileTreeEntry = (file: ClientFile) => (
    <div
      key={file.path}
      className={clsx(fileTreeEntryClassName, 'hover:bg-gray-700', {
        'bg-gray-700': fileTicked(file),
        'bg-gray-800': !fileTicked(file),
      })}
    >
      <div
        className={clsx(
          '-my-2 -mx-2.5 flex h-8 w-8 shrink-0 cursor-pointer select-none items-center justify-center overflow-clip rounded-full hover:bg-gray-faded/30',
          fileTicked(file) && 'text-gray-300 hover:text-gray-300',
          !fileTicked(file) && 'text-gray-400 hover:text-gray-300'
        )}
        onClick={(e: React.MouseEvent) => {
          e.stopPropagation();
          tickFile(file, !fileTicked(file));
        }}
      >
        <FontAwesomeIcon icon={fileTicked(file) ? faCheckSquare : faSquare} />
      </div>
      <div className="w-3">
        {file.file_type === 'Directory' && (
          <FontAwesomeIcon icon={faFolder} className="text-blue-accent" />
        )}
        {file.file_type === 'File' && (
          <FontAwesomeIcon
            icon={openedFile?.path === file.path ? faFilePen : faFile}
            className="text-gray-400"
          />
        )}
        {file.file_type === 'Unknown' && (
          <FontAwesomeIcon icon={faClipboardQuestion} className="text-ochre" />
        )}
      </div>
      <p
        className={clsx(
          'truncate text-gray-300 hover:cursor-pointer hover:text-blue-accent hover:underline',
          openedFile?.path === file.path && 'italic'
        )}
        onClick={() => {
          if (file.file_type === 'Directory') {
            setPath(file.path);
            setOpenedFile(null);
            setTickedFiles([]);
          } else {
            setOpenedFile(file);
          }
        }}
      >
        {file.name}
      </p>
      <div className="grow"></div>

      <p className="hidden min-w-[10ch] text-right text-gray-500 @xs:inline">
        {file.modification_time || file.creation_time
          ? formatTimeAgo(
              Number(file.modification_time ?? file.creation_time) * 1000
            )
          : 'Unknown Time'}
      </p>
    </div>
  );
  const fileTree = (
    <div className="flex h-full w-full grow flex-col @container/file-tree">
      <div className="overflow-y-overlay flex h-0 grow flex-col divide-y divide-gray-faded/30 overflow-x-hidden">
        {!atTopLevel ? (
          <div
            key={'..'}
            className="group flex flex-row items-center gap-4 bg-gray-800 py-2 px-4 hover:cursor-pointer hover:bg-gray-700 hover:text-blue-accent hover:underline"
            onClick={() => {
              setPath(parentPath);
              setOpenedFile(null);
              setTickedFiles([]);
            }}
          >
            <p className="select-none text-base font-medium">..</p>
          </div>
        ) : null}

        {fileListLoading ? (
          <div className={fileTreeEntryClassName}>
            <p className="text-base font-medium text-gray-400">Loading...</p>
          </div>
        ) : fileListError ? (
          <div className={fileTreeEntryClassName}>
            <p className="text-base font-medium text-gray-400">
              {fileListError.message}
            </p>
          </div>
        ) : null}

        {fileList?.length === 0 && (
          <div className={fileTreeEntryClassName}>
            <p className="text-base font-medium text-gray-400">
              No files here...
            </p>
          </div>
        )}
        {fileList?.map(fileTreeEntry)}
        <div
          onClick={() => setOpenedFile(null)}
          className="min-h-[25%] grow"
        ></div>
      </div>
    </div>
  );

  const createFileModal = (
    <Dialog
      open={createFileModalOpen}
      onClose={() => setCreateFileModalOpen(false)}
    >
      <div className="fixed inset-0 bg-[#000]/80" />
      <div className="fixed inset-0 overflow-y-auto">
        <div className="flex min-h-full items-center justify-center p-4 text-center">
          <Dialog.Panel>
            <div className="flex w-[500px] flex-col items-stretch justify-center gap-12 rounded-3xl bg-gray-800 px-8 pb-8 pt-16">
              <Formik
                initialValues={{ name: '' }}
                validationSchema={yup.object({
                  name: yup.string().required('Required'),
                })}
                onSubmit={async (values: { name: string }, actions: any) => {
                  actions.setSubmitting(true);
                  const error = await createInstanceFile(
                    instance.uuid,
                    path,
                    values.name
                  );
                  if (error) {
                    actions.setErrors({ name: error });
                    actions.setSubmitting(false);
                  } else {
                    queryClient.setQueriesData(
                      ['instance', instance.uuid, 'fileList', path],
                      fileList
                        ? [
                            ...fileList,
                            {
                              name: values.name,
                              path: `${path}/${values.name}`,
                              file_type: 'File' as FileType,
                              creation_time: Date.now() / 1000,
                              modification_time: Date.now() / 1000,
                            },
                          ].sort(fileSorter)
                        : undefined
                    );
                    actions.setSubmitting(false);
                    actions.resetForm();
                    setCreateFileModalOpen(false);
                  }
                }}
              >
                {({ isSubmitting }) => (
                  <Form
                    id="create-file-form"
                    autoComplete="off"
                    className="flex flex-col items-stretch gap-8 text-center"
                  >
                    <InputField
                      name="name"
                      label="Name your file"
                      placeholder="Untitled"
                    />
                    <div className="flex flex-row justify-between">
                      <Button
                        onClick={() => setCreateFileModalOpen(false)}
                        label="Cancel"
                      />
                      <Button
                        type="submit"
                        label="Create file"
                        loading={isSubmitting}
                      />
                    </div>
                  </Form>
                )}
              </Formik>
            </div>
          </Dialog.Panel>
        </div>
      </div>
    </Dialog>
  );

  const createFolderModal = (
    <Dialog
      open={createFolderModalOpen}
      onClose={() => setCreateFolderModalOpen(false)}
    >
      <div className="fixed inset-0 bg-[#000]/80" />
      <div className="fixed inset-0 overflow-y-auto">
        <div className="flex min-h-full items-center justify-center p-4 text-center">
          <Dialog.Panel>
            <div className="flex w-[500px] flex-col items-stretch justify-center gap-12 rounded-3xl bg-gray-800 px-8 pb-8 pt-16">
              <Formik
                initialValues={{ name: '' }}
                validationSchema={yup.object({
                  name: yup.string().required('Required'),
                })}
                onSubmit={async (values: { name: string }, actions: any) => {
                  actions.setSubmitting(true);
                  const error = await createInstanceDirectory(
                    instance.uuid,
                    path,
                    values.name
                  );
                  if (error) {
                    actions.setErrors({ name: error });
                    actions.setSubmitting(false);
                  } else {
                    queryClient.setQueriesData(
                      ['instance', instance.uuid, 'fileList', path],
                      fileList
                        ? [
                            ...fileList,
                            {
                              name: values.name,
                              path: `${path}/${values.name}`,
                              file_type: 'Directory' as FileType,
                              creation_time: Date.now() / 1000,
                              modification_time: Date.now() / 1000,
                            },
                          ].sort(fileSorter)
                        : undefined
                    );
                    actions.setSubmitting(false);
                    actions.resetForm();
                    setCreateFolderModalOpen(false);
                  }
                }}
              >
                {({ isSubmitting }) => (
                  <Form
                    id="create-folder-form"
                    autoComplete="off"
                    className="flex flex-col items-stretch gap-8 text-center"
                  >
                    <InputField
                      name="name"
                      label="Name your folder"
                      placeholder="Untitled folder"
                    />
                    <div className="flex flex-row justify-between">
                      <Button
                        onClick={() => setCreateFolderModalOpen(false)}
                        label="Cancel"
                      />
                      <Button
                        type="submit"
                        label="Create folder"
                        loading={isSubmitting}
                      />
                    </div>
                  </Form>
                )}
              </Formik>
            </div>
          </Dialog.Panel>
        </div>
      </div>
    </Dialog>
  );

  return (
    <div className="flex h-full w-full flex-col gap-3">
      <div className="flex flex-row items-center justify-between gap-4">
        {createFileModal}
        {createFolderModal}
        <Menu as="div" className="relative inline-block text-left">
          <Menu.Button
            as={Button}
            label="Add/Remove"
            icon={faAngleDown}
          ></Menu.Button>
          <Transition
            as={Fragment}
            enter="transition ease-out duration-200"
            enterFrom="opacity-0 -translate-y-1"
            enterTo="opacity-100 translate-y-0"
            leave="transition ease-in duration-150"
            leaveFrom="opacity-100 translate-y-0"
            leaveTo="opacity-0 -translate-y-1"
          >
            <Menu.Items className="absolute -left-0.5 z-10 mt-2 origin-top-left divide-y divide-gray-faded/30 rounded border border-gray-faded/30 bg-gray-800 drop-shadow-md focus:outline-none">
              <div className="p-1">
                <Menu.Item>
                  {({ active, disabled }) => (
                    <Button
                      label="Create new file"
                      className="w-full items-start whitespace-nowrap py-1.5 font-normal"
                      onClick={() => setCreateFileModalOpen(true)}
                      icon={faPlus}
                      variant="text"
                      align="start"
                      size="slim"
                      disabled={disabled}
                      active={active}
                    />
                  )}
                </Menu.Item>
                <Menu.Item>
                  {({ active, disabled }) => (
                    <Button
                      label="Create new folder"
                      className="w-full items-start whitespace-nowrap py-1.5 font-normal"
                      onClick={() => setCreateFolderModalOpen(true)}
                      icon={faFolderPlus}
                      variant="text"
                      align="start"
                      size="slim"
                      disabled={disabled}
                      active={active}
                    />
                  )}
                </Menu.Item>
              </div>
              <div className="p-1">
                <Menu.Item disabled={tickedFiles.length === 0}>
                  {({ active, disabled }) => (
                    <Button
                      className="w-full items-start whitespace-nowrap py-1.5 font-normal"
                      label="Download"
                      icon={faDownload}
                      onClick={downloadTickedFiles}
                      variant="text"
                      align="start"
                      size="slim"
                      disabled={disabled}
                      active={active}
                    />
                  )}
                </Menu.Item>
              </div>
              <div className="p-1">
                <Menu.Item disabled={tickedFiles.length === 0}>
                  {({ active, disabled }) => (
                    <Button
                      label="Delete selected"
                      className="w-full items-start whitespace-nowrap py-1.5 font-normal"
                      onClick={deleteTickedFiles}
                      icon={faTrashCan}
                      variant="text"
                      align="start"
                      color="red"
                      size="slim"
                      disabled={disabled}
                      active={active}
                    />
                  )}
                </Menu.Item>
              </div>
            </Menu.Items>
          </Transition>
        </Menu>

        {breadcrumb}
        {showingMonaco && (
          <Button
            className="h-fit"
            label="Save"
            icon={faFloppyDisk}
            onClick={() =>
              saveInstanceFile(
                instance.uuid,
                path,
                openedFile as any, //force ignore "null" possibility
                fileContent,
                queryClient
              )
            }
            disabled={
              !openedFile ||
              fileContent === originalFileContent ||
              !showingMonaco
            }
          />
        )}
        <Button
          className="h-fit"
          label="Upload"
          icon={faUpload}
          onClick={chooseFilesToUpload}
        />
      </div>
      <div className="flex h-full w-full flex-row divide-x divide-gray-faded/30 overflow-clip rounded-lg border border-gray-faded/30 bg-gray-800">
        <ResizePanel
          direction="e"
          maxSize={500}
          minSize={200}
          size={fileListSize}
          validateSize={false}
          onResize={setFileListSize}
          containerClassNames="grow"
          grow={!openedFile}
        >
          {fileTree}
        </ResizePanel>
        {openedFile && (
          <div className="min-w-0 grow">
            <div className="h-full">
              {showingMonaco ? (
                <Editor
                  height="100%"
                  onChange={(value) => {
                    setfileContent(value ?? '');
                  }}
                  value={fileContent}
                  theme="lodestone-dark"
                  path={monacoPath}
                  className="overflow-clip bg-gray-800"
                  options={{
                    padding: {
                      top: 8,
                    },
                    minimap: {
                      enabled: false,
                    },
                  }}
                  onMount={handleEditorDidMount}
                />
              ) : (
                <div className="flex h-full w-full flex-col items-center justify-center gap-4 bg-gray-800">
                  <FontAwesomeIcon
                    icon={faFilePen}
                    className="text-xlarge text-gray-500"
                  />
                  <p className="text-xl text-center text-gray-400">
                    File Editor
                  </p>
                  <p className="text-xl text-center text-gray-400">
                    {fileError
                      ? fileError?.message ?? 'Unknown Error'
                      : isFileLoading
                      ? 'Loading...'
                      : 'Select a file to view its contents'}
                  </p>
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
