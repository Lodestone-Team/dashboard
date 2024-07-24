import { faClipboardQuestion } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { ClientFile } from 'bindings/ClientFile';
import clsx from 'clsx';
import Checkbox from 'components/Atoms/Checkbox';
import { formatBytes, formatTimeAgo, supportedZip } from 'utils/util';
import FileContextMenu from './FileContextMenu';
import React, { useState, useEffect, useRef } from 'react';
import { useEventListener, useOnClickOutside } from 'usehooks-ts';
import { UnzipOption } from 'bindings/UnzipOptions';

// a map from file extension to icon
const iconMap: { [key: string]: string } = {
  txt: '/icons/document.svg',
  md: '/icons/document.svg',
  properties: '/icons/document.svg',
  yml: '/icons/yaml.svg',
  yaml: '/icons/yaml.svg',
  exe: '/icons/exe.svg',
  cfg: '/icons/document.svg',
  conf: '/icons/document.svg',
  config: '/icons/document.svg',
  ini: '/icons/document.svg',
  png: '/icons/image.svg',
  jpg: '/icons/image.svg',
  jpeg: '/icons/image.svg',
  gif: '/icons/image.svg',
  bmp: '/icons/image.svg',
  svg: '/icons/image.svg',
  json: '/icons/json.svg',
  mcmeta: '/icons/json.svg',
  zip: '/icons/zip.svg',
  gz: '/icons/zip.svg',
  tar: '/icons/zip.svg',
  jar: '/icons/jar.svg',
  java: '/icons/java.svg',
  class: '/icons/javaclass.svg',
  ts: '/icons/typescript.svg',
  js: '/icons/javascript.svg',
  readme: '/icons/readme.svg',
  bin: '/icons/binary.svg',
  dll: '/icons/binary.svg',
  dylib: '/icons/binary.svg',
  so: '/icons/binary.svg',
  o: '/icons/binary.svg',
  dat: '/icons/3d.svg',
  dat_old: '/icons/3d.svg',
  obj: '/icons/3d.svg',
  nbt: '/icons/3d.svg',
  mca: '/icons/3d.svg',
  lock: '/icons/lock.svg',
  log: '/icons/log.svg',
  ps1: '/icons/pwsh.svg',
  sh: '/icons/console.svg',
  mcfunction: '/icons/minecraft.svg',
};

export default function FileList({
  path,
  fileList,
  loading,
  error,
  tickedFiles,
  clipboard,
  tickFile,
  zipFiles,
  unzipFile,
  openedFile,
  atTopLevel,
  onParentClick,
  onEmptyClick,
  onFileClick,
  setCreateFolderModalOpen,
  setCreateFileModalOpen,
  setModalPath,
  setClipboard,
  setClipboardAction,
  pasteFiles,
  clipboardAction,
  setRenameFileModalOpen,
  setTickedFiles,
  deleteSingleFile,
  deleteTickedFiles,
}: {
  path: string;
  fileList: ClientFile[] | undefined;
  clipboard: ClientFile[] | undefined;
  loading: boolean;
  error: Error | null;
  tickedFiles: ClientFile[];
  tickFile: (file: ClientFile, ticked: boolean) => void;
  zipFiles: (files: ClientFile[], dest: string) => void;
  unzipFile: (file: ClientFile, unzipOption: UnzipOption) => void;
  openedFile: ClientFile | null;
  atTopLevel: boolean;
  onParentClick: () => void;
  onEmptyClick: () => void;
  pasteFiles: (path: string) => void;
  onFileClick: (file: ClientFile) => void;
  setCreateFileModalOpen: (modalOpen: boolean) => void;
  setRenameFileModalOpen: (modalOpen: boolean) => void;
  setCreateFolderModalOpen: (modalOpen: boolean) => void;
  setModalPath: (modalPath: string) => void;
  setClipboard: (clipboard: ClientFile[]) => void;
  clipboardAction: 'copy' | 'cut';
  setClipboardAction: (clipboardAction: 'copy' | 'cut') => void;
  setTickedFiles: (tickedFiles: ClientFile[]) => void;
  deleteSingleFile: (file: ClientFile) => void;
  deleteTickedFiles: () => void;
}) {
  const fileTicked = (file: ClientFile) => {
    // check just the path and type, not other metadata
    return tickedFiles.some(
      (f) => f.path === file.path && f.file_type === file.file_type
    );
  };

  const boundingDivRef = useRef<HTMLDivElement>(null);
  const contextMenuRef = useRef<HTMLDivElement>(null);
  const [mousePos, setMousePos] = useState<{ x: number; y: number }>({
    x: 0,
    y: 0,
  });
  const [sortUpdate, setSortUpdate] = useState(false);
  const [sortKey, setSortKey] = useState<{ key: string, direction: number}>({
    key: '',
    direction: -1 // 1 for asc, -1 for desc
  })
  const [showContextMenu, setShowContextMenu] = useState(false);
  const [contextMenuCoords, setContextMenuCoords] = useState({ x: 0, y: 0 });
  const [contextMenuFile, setContextMenuFile] = useState<ClientFile | null>();
  const [absCoords, setAbsCoords] = useState({ x: 0, y: 0 });
  const [boundingDivDimensions, setBoundingDivDimensions] = useState({
    height: 0,
    width: 0,
  });

  const contextMenuDimensionsWithoutUnzip = { height: 259, width: 176 }; // no unzip in menu (file name is null)
  const contextMenuDimensionsWithUnzip = { height: 337, width: 176 };

  useEffect(() => {
    if (boundingDivRef.current !== null) {
      setBoundingDivDimensions({
        height: boundingDivRef.current.offsetHeight,
        width: boundingDivRef.current.offsetWidth,
      });
    }
  }, [boundingDivRef]);

  useEffect(() => {
    if (boundingDivRef.current !== null) {
      setAbsCoords({
        x: boundingDivRef.current.getBoundingClientRect().left + window.scrollX,
        y: boundingDivRef.current.getBoundingClientRect().top + window.scrollY,
      });
    }
  }, []);

  useEffect(() => {
    if (sortKey.key == '') return;
    fileList?.sort((a, b) => {
        const { direction, key } = sortKey;
        
        // group directories together
        if (a.file_type === 'Directory' && b.file_type !== 'Directory') return -1 * direction;
        if (a.file_type !== 'Directory' && b.file_type === 'Directory') return 1 * direction;
        
        const compare = (val1: string | number | null, val2: string | number | null) => {
            if (!val1) return -1;
            if (!val2) return 1;
            return val1 < val2 ? -1 : 1;
        };

        let result = 0;
        if (key === 'name') result = compare(a.name, b.name);
        else if (key === 'date_modified') result = compare(a.modification_time, b.modification_time);
        else if (key === 'size') result = compare(a.size, b.size);
        return direction === 1 ? result : -result;
    });
    setSortUpdate(!sortUpdate);

  }, [sortKey, fileList]);

  const onMouseMove = (e: MouseEvent) => {
    setMousePos({ x: e.clientX - absCoords.x, y: e.clientY - absCoords.y });
  };

  const onResize = () => {
    if (boundingDivRef.current !== null) {
      setAbsCoords({
        x: boundingDivRef.current.getBoundingClientRect().left + window.scrollX,
        y: boundingDivRef.current.getBoundingClientRect().top + window.scrollY,
      });
      setBoundingDivDimensions({
        height: boundingDivRef.current.offsetHeight,
        width: boundingDivRef.current.offsetWidth,
      });
    }
  };

  useEventListener('mousemove', onMouseMove);
  useEventListener('resize', onResize);
  useEventListener('mousedown', onResize);
  useOnClickOutside(contextMenuRef, () => setShowContextMenu(false));

  function fileToIcon(file: ClientFile) {
    if (file.file_type === 'Directory') {
      return (
        <img src="/icons/folder-blue.svg" alt="folder icon" draggable="false" />
      );
    } else if (file.file_type === 'File') {
      if (file.file_stem === '.lodestone_config') {
        return (
          <img
            src="/icons/lodestone.svg"
            alt="file icon"
            draggable="false"
            // because the lodestone icon is a bit bigger than the others
            // set between 4 and 5
            className="h-4 w-4"
          />
        );
      }
      const fileExt = file.extension ?? '';
      if (fileExt in iconMap) {
        return (
          <img
            src={iconMap[fileExt]}
            alt="file icon"
            draggable="false"
            className="h-4 w-4"
          />
        );
      } else {
        return (
          <img
            src="/icons/file.svg"
            alt="file icon"
            draggable="false"
            className="h-4 w-4"
          />
        );
      }
    }

    return (
      <FontAwesomeIcon icon={faClipboardQuestion} className="text-yellow" />
    );
  }

  const calculateContextMenuCoords = (file?: ClientFile) => {
    let x = null;
    let y = null;
    let width = 0;
    let height = 0;
    const fileExt = file?.extension ?? '';

    if (supportedZip.includes(fileExt)) {
      width = contextMenuDimensionsWithUnzip.width;
      height = contextMenuDimensionsWithUnzip.height;
    } else {
      width = contextMenuDimensionsWithoutUnzip.width;
      height = contextMenuDimensionsWithoutUnzip.height;
    }

    if (mousePos.x + width > boundingDivDimensions.width) {
      x = mousePos.x - width;
    } else {
      x = mousePos.x;
    }
    if (mousePos.y + height > boundingDivDimensions.height - 10) {
      y = boundingDivDimensions.height - height - 10;
    } else {
      y = mousePos.y;
    }
    if (
      mousePos.x + width > boundingDivDimensions.width &&
      mousePos.x - width < 4
    ) {
      x = 4;
    }

    setContextMenuCoords({ x, y });
  };

  const fileTreeEntryClassName =
    'flex flex-row items-center gap-4 py-2 px-4 text-medium font-medium tracking-medium whitespace-nowrap';

  return (
    <div
      className="flex h-full w-full grow flex-col @container/file-tree"
      ref={boundingDivRef}
    >
      {showContextMenu && (
        <FileContextMenu
          ref={contextMenuRef}
          currentPath={path}
          file={contextMenuFile as ClientFile}
          coords={contextMenuCoords}
          setCreateFileModalOpen={setCreateFileModalOpen}
          setRenameFileModalOpen={setRenameFileModalOpen}
          setCreateFolderModalOpen={setCreateFolderModalOpen}
          setShowContextMenu={setShowContextMenu}
          setClipboard={setClipboard}
          zipFiles={zipFiles}
          unzipFile={unzipFile}
          setModalPath={setModalPath}
          setClipboardAction={setClipboardAction}
          setTickedFiles={setTickedFiles}
          tickedFiles={tickedFiles}
          clipboard={clipboard}
          pasteFiles={pasteFiles}
          deleteSingleFile={deleteSingleFile}
          deleteTickedFiles={deleteTickedFiles}
        />
      )}
      <div className="overflow-y-overlay flex h-0 grow flex-col divide-y divide-gray-faded/30 overflow-x-hidden">
        {!atTopLevel ? (
          <div
            key={'..'}
            className="group flex flex-row items-center gap-4 bg-gray-800 py-2 px-4 hover:cursor-pointer hover:bg-gray-700 hover:text-blue-200 hover:underline"
            onClick={onParentClick}
          >
            <p className="select-none text-medium font-medium">..</p>
          </div>
        ) : null}

        {loading ? (
          <div className={fileTreeEntryClassName}>
            <p className="text-medium font-medium text-gray-400">Loading...</p>
          </div>
        ) : error ? (
          <div className={fileTreeEntryClassName}>
            <p className="text-medium font-medium text-gray-400">
              {error.message}
            </p>
          </div>
        ) : null}

        {fileList?.length === 0 && (
          <div className={fileTreeEntryClassName}>
            <p className="text-medium font-medium text-gray-400">
              No files here...
            </p>
          </div>
        )}
        <div className='text-medium flex w-full flex-row font-medium text-gray-400'>
            <p className='py-2 pl-11 hover:cursor-pointer' 
                onClick={() => setSortKey({ key: 'name', direction: sortKey.direction * -1})}>
                Name
            </p>
            <div className="grow"></div>
            <p className='min-w-[8ch] py-2 pr-8 text-right hover:cursor-pointer' 
                onClick={() => setSortKey({ key: 'date_modified', direction: sortKey.direction * -1})}>
                Date modified
            </p>
            <p className='min-w-[8ch] py-2 pr-4 text-right hover:cursor-pointer'
                onClick={() => setSortKey({ key: 'size', direction: sortKey.direction * -1})}>
                Size
            </p>
        </div>
        {fileList?.map((file: ClientFile) => (
          <div
            key={file.path}
            className={clsx(fileTreeEntryClassName, 'hover:bg-gray-700', {
              'bg-gray-900': file === openedFile,
              'bg-gray-700': fileTicked(file) && file !== openedFile,
              'bg-gray-800': !fileTicked(file) && file !== openedFile,
            })}
            onContextMenu={(e) => {
              e.preventDefault();
              setContextMenuFile(file);
              calculateContextMenuCoords(file);
              setShowContextMenu(true);
              setModalPath(file.file_type === 'Directory' ? file.path : path);
            }}
          >
            <Checkbox
              checked={fileTicked(file)}
              onChange={(ticked) => {
                tickFile(file, ticked);
              }}
            />
            <div className="h-4 w-4 flex-shrink-0">{fileToIcon(file)}</div>
            <p
              className={clsx(
                'truncate text-gray-300 hover:cursor-pointer hover:text-blue-200 hover:underline',
                openedFile?.path === file.path && 'italic',
                clipboardAction === 'cut' &&
                  clipboard?.some((f) => f.path === file.path) &&
                  'text-opacity-60 '
              )}
              onClick={() => onFileClick(file)}
            >
              {file.name}
            </p>
            <div className="grow"></div>

            <p className="hidden min-w-[8ch] text-left text-gray-500 @xs:inline">
              {file.modification_time || file.creation_time
                ? formatTimeAgo(Number(file.modification_time ?? file.creation_time) * 1000)
                : 'Unknown Creation Time'}
            </p>

            <p className="hidden min-w-[8ch] text-right text-gray-500 @sm:inline">
              {file.file_type === 'Directory'
                ? ''
                : file.size
                ? formatBytes(file.size, 1)
                : ''}
            </p>
          </div>
        ))}
        <div
          onClick={onEmptyClick}
          className="min-h-[25%] grow"
          onContextMenu={(e) => {
            e.preventDefault();
            setContextMenuFile(null);
            calculateContextMenuCoords();
            setShowContextMenu(true);
            setModalPath(path);
          }}
        ></div>
      </div>
    </div>
  );
}
