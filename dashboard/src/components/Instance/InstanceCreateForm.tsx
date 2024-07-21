import Button from 'components/Atoms/Button';
import { Form, Formik, FormikHelpers, FormikProps } from 'formik';
import { useRef, useState, useEffect, useMemo, useContext } from 'react';
import { useEffectOnce } from 'usehooks-ts';
import useAnalyticsEventTracker from 'utils/hooks';
import { axiosWrapper, catchAsyncToString, chooseFile, chooseFiles } from 'utils/util';
import {
  autoSettingPageObject,
  basicSettingsPageObject,
  formId,
} from './Create/form';
import { generateValidationSchema, generateInitialValues } from './Create/form';
import { FormFromManifest } from './Create/FormFromManifest';
import GameTypeSelectForm from './Create/GameTypeSelectForm';
import {
  SetupGenericInstanceManifest,
  SetupInstanceManifest,
} from 'data/InstanceGameTypes';
import { HandlerGameType } from 'bindings/HandlerGameType';
import Spinner from 'components/DashboardLayout/Spinner';
import WarningAlert from 'components/Atoms/WarningAlert';
import clsx from 'clsx';
import * as yup from 'yup';
import { GameInstanceContext } from 'data/GameInstanceContext';
import { ConfigurableValue } from 'bindings/ConfigurableValue';
import { SetupManifest } from 'bindings/SetupManifest';
import { SetupValue } from 'bindings/SetupValue';
import { SectionManifestValue } from 'bindings/SectionManifestValue';
import { toast } from 'react-toastify';
import { faUpload } from '@fortawesome/free-solid-svg-icons';
import { Base64 } from 'js-base64';
import { getTmpPath, uploadFile } from 'utils/apis';
import { NotificationContext } from 'data/NotificationContext';

export type GenericHandlerGameType = 'Generic' | HandlerGameType;
export type FormPage = {
  name: string;
  description: string;
  page: SetupManifest;
};

let submitAction: string | undefined = undefined;

export default function InstanceCreateForm({
  onComplete,
}: {
  onComplete: () => void;
}) {
  const [activeStep, setActiveStep] = useState(0);
  const [gameType, setGameType] = useState<GenericHandlerGameType>('Generic');
  const [genericFetchReady, setGenericFetchReady] = useState(false); //if the button has been pressed to fetch the manifest -> enables the query
  const [urlValid, setUrlValid] = useState(false); //if the query returned a valid manifest
  const [url, setUrl] = useState<string>(''); //the url the user enters
  const [setupManifest, setSetupManifest] = useState<SetupManifest | null>(
    null
  );
  const { ongoingNotifications } = useContext(NotificationContext);

  const {
    data: setup_manifest,
    isLoading,
    error,
  } = gameType === 'Generic'
      ? SetupGenericInstanceManifest(gameType, url, genericFetchReady)
      : SetupInstanceManifest(gameType as HandlerGameType);

  const gaEventTracker = useAnalyticsEventTracker('Create Instance');
  const formikRef =
    useRef<FormikProps<Record<string, ConfigurableValue | null>>>(null);

  const formPages = useMemo<FormPage[]>(() => {
    if (!setupManifest)
      return [
        {
          name: 'Basic Settings',
          description: 'Basic settings for your server.',
          page: { setting_sections: { section_1: basicSettingsPageObject } },
        },
      ];

    return [
      {
        name: 'Basic Settings',
        description: 'Basic settings for your server.',
        page: { setting_sections: { section_1: basicSettingsPageObject } },
      },
      {
        name: 'Instance Settings',
        description: 'Configure your server.',
        page: setupManifest,
      },
      {
        name: 'Auto Settings',
        description: 'Automatically configure your server.',
        page: { setting_sections: { section_1: autoSettingPageObject } },
      },
    ];
  }, [setupManifest]);

  useEffectOnce(() => {
    gaEventTracker('Create Instance Start');
  });

  useEffect(() => {
    if (gameType !== 'Generic') {
      setGenericFetchReady(false);
      setUrlValid(false);
    }
    if (!isLoading && !error) {
      if (gameType === 'Generic' && genericFetchReady) setUrlValid(true); //value fetched with no errors (this is to cover the initial case when nothing has been fetched yet)
      setInitialValues(
        generateInitialValues(setup_manifest['setting_sections'])
      );
      setValidationSchema(generateValidationSchema(setup_manifest));
      setSetupManifest(setup_manifest);
    }
  }, [gameType, isLoading, setup_manifest, error, genericFetchReady]);



  const [initialValues, setInitialValues] = useState<
    Record<string, ConfigurableValue | null>
  >({});
  const [validationSchema, setValidationSchema] = useState<any[]>([
    yup.object().shape({}),
  ]);

  // if (setupManifest === null && activeStep !== 0) return <Spinner />;
  const currentValidationSchema = validationSchema[activeStep];

  const sections = [
    'Select Game',
    'Basic Settings',
    'Instance Settings',
    'Auto Settings',
  ];
  const formReady = activeStep === sections.length - 1;
  const createInstance = async (value: SetupValue) => {
    try {
      if (gameType === 'Generic') {
        await axiosWrapper<void>({
          method: 'post',
          url: `/instance/create_generic`,
          headers: { 'Content-Type': 'application/json' },
          data: JSON.stringify({ url: url, setup_value: value }),
        });
      } else {
        await axiosWrapper<void>({
          method: 'post',
          url: `/instance/create/${gameType}`,
          headers: { 'Content-Type': 'application/json' },
          data: JSON.stringify(value),
        });
      }
    } catch (e) {
      toast.error('Error creating instance: ' + e);
    }
  };

  const uploadInstance = async (value: SetupValue, path: string) => {
    path = Base64.encode(path, true)
    console.log(path, Base64.decode(path))


    try {
      await axiosWrapper<void>({
        method: 'post',
        url: `/instance/create_from_zip/${gameType}/${path}`,
        headers: { 'Content-Type': 'application/json' },
        data: JSON.stringify(value),
      });
    } catch (e) {
      toast.error('Error creating instance: ' + e);
    }
  };


  async function submitForm(
    values: Record<string, ConfigurableValue | null>,
    actions: FormikHelpers<Record<string, ConfigurableValue | null>>,
    path: string | null = null
  ) {
    const sectionValues = parseValues(values);

    const parsedValues: SetupValue = {
      name: values.name?.value as string,
      description: values.description?.value as string,
      auto_start: values.auto_start?.value as boolean,
      restart_on_crash: values.restart_on_crash?.value as boolean,
      setting_sections: sectionValues,
    };

    if (submitAction == "upload" && path)
      await uploadInstance(parsedValues, path);
    else
      await createInstance(parsedValues);
    actions.setSubmitting(false);

    gaEventTracker('Create Instance Complete');
    onComplete();
  }

  function parseValues(
    values: Record<string, ConfigurableValue | null>
  ): Record<string, SectionManifestValue> {
    if (!setup_manifest) return {};
    const sectionKeys = Object.keys(setup_manifest['setting_sections']);

    const sectionValues: Record<string, SectionManifestValue> = {};

    for (const sectionKey of sectionKeys) {
      const sectionValue: SectionManifestValue = { settings: {} };
      const settingKeys = Object.keys(
        setup_manifest['setting_sections'][sectionKey]['settings']
      );
      for (const key of settingKeys) {
        sectionValue['settings'][key] = { value: values[key] };
      }
      sectionValues[sectionKey] = sectionValue;
    }

    return sectionValues;
  }

  async function handleSubmit(
    values: Record<string, ConfigurableValue | null>,
    actions: FormikHelpers<Record<string, ConfigurableValue | null>>,
  ) {
    if (formReady) {
      if (submitAction === "upload") {
        const file = await chooseFiles(false);
        if (!file) {
          return;
        }
        const fileArray = Array.from(file);
        const tmpDir = await getTmpPath();
        let directorySeparator = '\\';
        // assume only linux paths contain /
        if (tmpDir.includes('/')) directorySeparator = '/';
        uploadFile(tmpDir, fileArray, true);
        submitForm(values, actions, tmpDir + directorySeparator + fileArray[0].name);
        onComplete();
      } else {
        submitForm(values, actions);
        onComplete();
      }
    } else {
      if (setup_manifest) {
        if (activeStep === 0) actions.setValues(initialValues);
        setActiveStep(activeStep + 1);
      }
      actions.setTouched({});
      actions.setSubmitting(false);
    }
  }

  function handleBack() {
    if (activeStep === 1) {
      setGenericFetchReady(false);
      setUrlValid(false);
      setUrl('');
    }
    setActiveStep(activeStep - 1);
  }

  return (
    <GameInstanceContext.Provider
      value={{
        gameType: gameType,
        setGameType: setGameType,
        url: url,
        setUrl: setUrl,
        urlValid: urlValid,
        setUrlValid: setUrlValid,
        genericFetchReady: genericFetchReady,
        setGenericFetchReady: setGenericFetchReady,
      }}
    >
      <Formik
        initialValues={initialValues}
        validationSchema={currentValidationSchema}
        onSubmit={handleSubmit}
        innerRef={formikRef}
        validateOnBlur={false}
        validateOnChange={false}
      >
        {({ isSubmitting, status }) => (
          <Form
            id={formId}
            className="border-gray-faded/10 bg-gray-850 flex max-h-[700px] min-h-[560px] w-[812px] rounded-2xl border-2 drop-shadow-lg"
          >
            <div className="w-[180px] border-r border-gray-700 pt-8 ">
              {sections.map((section, i) => (
                <div
                  key={i}
                  className={clsx(
                    'text-medium tracking-medium px-4 py-2 text-left font-sans font-medium leading-5 text-white/50 ',
                    activeStep === i && 'font-extrabold text-white'
                  )}
                >
                  {section}
                </div>
              ))}
            </div>
            <div className="flex w-[632px] flex-col p-8">
              {activeStep == 0 ? (
                <GameTypeSelectForm
                  manifestLoading={isLoading}
                  manifestError={!!error}
                />
              ) : (
                <FormFromManifest
                  name={formPages[activeStep - 1].name}
                  description={formPages[activeStep - 1].description}
                  manifest={formPages[activeStep - 1].page}
                >
                  {status && (
                    <WarningAlert>
                      <p>
                        Please ensure your fields are filled out correctly
                        <br />
                        {status.error}
                      </p>
                    </WarningAlert>
                  )}
                </FormFromManifest>
              )}
              <div className="flex flex-row justify-between pt-6">
                {activeStep !== 0 ? (
                  <Button onClick={handleBack} label="Back" />
                ) : (
                  <div></div>
                )}
                <div className="m-0 flex flex-row p-0">
                  {formReady &&
                    <Button
                      type="submit"
                      onClick={() => submitAction = "upload"}
                      iconRight={faUpload}
                      label="Upload"
                      loading={isSubmitting}
                      disabled={gameType === 'Generic' && !urlValid}
                      className="mr-2"
                    />
                  }
                  <Button
                    type="submit"
                    label={formReady ? 'Create Instance' : 'Next'}
                    loading={isSubmitting}
                    disabled={gameType === 'Generic' && !urlValid}
                  />
                </div>
              </div>
            </div>
          </Form>
        )}
      </Formik>
    </GameInstanceContext.Provider>
  );
}
