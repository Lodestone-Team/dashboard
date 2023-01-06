import { useQueryClient } from '@tanstack/react-query';
import InputBox from 'components/Atoms/Config/InputBox';
import ToggleBox from 'components/Atoms/Config/ToggleBox';
import ConfirmDialog from 'components/Atoms/ConfirmDialog';
import { useGlobalSettings } from 'data/GlobalSettings';
import { LodestoneContext } from 'data/LodestoneContext';
import { useCoreInfo } from 'data/SystemInfo';
import { useUserInfo } from 'data/UserInfo';
import { useContext, useState } from 'react';
import { toast } from 'react-toastify';
import {
  axiosPutSingleValue,
  catchAsyncToString,
  errorToString,
} from 'utils/util';

export const CoreSettings = () => {
  const { core } = useContext(LodestoneContext);
  const queryClient = useQueryClient();
  const { data: globalSettings, isLoading, error } = useGlobalSettings();
  const { data: coreInfo } = useCoreInfo();
  const { data: userInfo } = useUserInfo();
  const can_change_core_settings = userInfo?.is_owner ?? false;
  const [showSafemodeDialog, setShowSafemodeDialog] = useState(false);
  // use a promise to track if safemode is successfully disabled

  const errorString = errorToString(error);
  // TODO: better form error displaying
  const nameField = (
    <InputBox
      label="Core Name"
      value={globalSettings?.core_name}
      isLoading={isLoading}
      error={errorString}
      disabled={!can_change_core_settings}
      canRead={userInfo !== undefined}
      description={
        'A nickname for this core. This is what you and others will see when you connect to this core'
      }
      validate={async (name) => {
        // don't be empty
        if (name === '') throw new Error('Name cannot be empty');
        // don't be too long
        if (name.length > 32)
          throw new Error('Name cannot be longer than 32 characters');
      }}
      onSubmit={async (name) => {
        await axiosPutSingleValue('/global_settings/name', name);
        queryClient.setQueryData(['global_settings'], {
          ...globalSettings,
          core_name: name,
        });
        queryClient.setQueryData(['systeminfo', 'CoreInfo'], {
          ...coreInfo,
          core_name: name,
        });
      }}
    />
  );

  const domainField = (
    <InputBox
      label="Public Domain/IP"
      value={globalSettings?.domain ?? ''}
      isLoading={isLoading}
      error={errorString}
      disabled={!can_change_core_settings}
      canRead={userInfo !== undefined}
      description={
        //TODO: more info needed once we add more functionality
        'The domain or public IP address of this core'
      }
      placeholder={`${core?.address}`}
      validate={async (domain) => {
        // can be empty
        if (domain === '') return;
        // don't be too long
        if (domain.length > 253)
          throw new Error('Domain cannot be longer than 253 characters');
      }}
      onSubmit={async (domain) => {
        await axiosPutSingleValue('/global_settings/domain', domain);
        queryClient.setQueryData(['global_settings'], {
          ...globalSettings,
          domain: domain,
        });
      }}
    />
  );

  const unsafeModeField = (
    <ToggleBox
      label={'Safe Mode'}
      value={globalSettings?.safe_mode ?? false}
      isLoading={isLoading}
      error={errorString}
      disabled={!can_change_core_settings}
      canRead={userInfo !== undefined}
      description={
        'Attempts to prevent non-owner users from accessing adminstrative permissions on your machine'
      }
      onChange={async (value) => {
        if (value) {
          await axiosPutSingleValue('/global_settings/safe_mode', value);
          queryClient.setQueryData(['global_settings'], {
            ...globalSettings,
            safe_mode: value,
          });
        } else {
          setShowSafemodeDialog(true);
        }
      }}
      optimistic={false}
    />
  );

  return (
    <>
      <ConfirmDialog
        title="Turn off safe mode?"
        isOpen={showSafemodeDialog}
        onConfirm={async () => {
          const error = await catchAsyncToString(
            axiosPutSingleValue('/global_settings/safe_mode', false)
          );
          if (error) {
            toast.error(error);
            return;
          }
          queryClient.setQueryData(['global_settings'], {
            ...globalSettings,
            safe_mode: false,
          });
          setShowSafemodeDialog(false);
        }}
        confirmButtonText="Turn off"
        onClose={() => setShowSafemodeDialog(false)}
        closeButtonText="Cancel"
        type={'info'}
      >
        Are you sure you want to turn off safe mode? This will allow all you to
        give users other than yourself the ability to run potentially dangerous
        commands. Make sure you trust all users you give these permissions to
      </ConfirmDialog>
      <div className="flex w-full flex-col gap-4 @4xl:flex-row">
        <div className="w-[28rem]">
          <h2 className="text-h2 font-bold tracking-tight"> Core Settings </h2>
          <h3 className="text-h3 font-medium italic tracking-normal text-white/50">
            These settings are for the core itself. They are not specific to any
            user
          </h3>
        </div>
        <div className="w-full rounded-lg border border-gray-faded/30 child:w-full child:border-b child:border-gray-faded/30 first:child:rounded-t-lg last:child:rounded-b-lg last:child:border-b-0">
          {nameField}
          {domainField}
          {unsafeModeField}
        </div>
      </div>
    </>
  );
};

export default CoreSettings;