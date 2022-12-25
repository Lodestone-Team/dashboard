import axios from 'axios';
import Button from 'components/Atoms/Button';
import { useContext, useEffect } from 'react';
import { DISABLE_AUTOFILL, errorToString } from 'utils/util';
import { LodestoneContext } from 'data/LodestoneContext';
import InputField from 'components/Atoms/Form/InputField';
import { Form, Formik, FormikHelpers } from 'formik';
import * as yup from 'yup';
import { faArrowLeft, faArrowRight } from '@fortawesome/free-solid-svg-icons';
import { BrowserLocationContext } from 'data/BrowserLocationContext';
import { useCoreInfo } from 'data/SystemInfo';

type SetupOwnerFormValues = {
  username: string;
  password: string;
  passwordConfirm: string;
  setupKey: string;
};

const validationSchema = yup.object({
  username: yup.string().required('Username is required'),
  password: yup.string().required('Password is required'),
  passwordConfirm: yup
    .string()
    .required('Password confirmation is required')
    .oneOf(
      [yup.ref('password'), null],
      'Password confirmation must match password'
    ),
  setupKey: yup.string().required('Setup key is required'),
});

const CoreSetupNew = () => {
  const { navigateBack, setPathname } = useContext(BrowserLocationContext);
  const { data: coreInfo } = useCoreInfo();
  const { setToken, core } = useContext(LodestoneContext);
  const { address, port } = core;
  const socket = `${address}:${port}`;

  useEffect(() => {
    if (coreInfo?.is_setup) {
      setPathname('/login/user/select');
    }
  }, [coreInfo]);

  const initialValues: SetupOwnerFormValues = {
    username: '',
    password: '',
    passwordConfirm: '',
    setupKey: '',
  };

  const onSubmit = (
    values: SetupOwnerFormValues,
    actions: FormikHelpers<SetupOwnerFormValues>
  ) => {
    // check if core can be reached
    axios
      .post(`/setup/${values.setupKey}`, {
        username: values.username,
        password: values.password,
      })
      .then((res) => {
        if (res.status !== 200)
          throw new Error('Something went wrong while setting up the core');
      })
      // .then(() => {
      //   loginToCore(values.password, values.username)
      //     .then((response) => {
      //       if (!response) {
      //         // this should never end
      //         actions.setErrors({ password: 'Login failed' });
      //         actions.setSubmitting(false);
      //         return;
      //       }
      //       setToken(response.token, socket);
      //       setPathname('/login/core/first_config');
      //       actions.setSubmitting(false);
      //     })
      //     .catch((error: string) => {
      //       actions.setErrors({ password: error });
      //       actions.setSubmitting(false);
      //     });
      // })
      .then(() => {
        setPathname('/login/user/select');
        actions.setSubmitting(false);
      })
      .catch((err) => {
        const errorMessages = errorToString(err);
        actions.setErrors({ setupKey: errorMessages }); //TODO: put the error in a better place, it's not just an address problem
        actions.setSubmitting(false);
        return;
      });
  };

  return (
    <div className="flex w-[768px] max-w-full flex-col items-stretch justify-center gap-12 rounded-3xl bg-gray-850 px-14 py-20 @container">
      <div className="text flex flex-col items-start">
        <img src="/logo.svg" alt="logo" className="h-9 w-40" />
        <h1 className="font-title text-2xlarge font-medium-semi-bold tracking-medium text-gray-300">
          Create an owner&#39;s account
        </h1>
        <h2 className="text-medium font-semibold tracking-medium text-white/50">
          Check the console output of the core to find the &#34;First time setup
          key&#34;.
        </h2>
      </div>
      <Formik
        initialValues={initialValues}
        validationSchema={validationSchema}
        onSubmit={onSubmit}
        validateOnBlur={false}
        validateOnChange={false}
      >
        {({ isSubmitting }) => (
          <Form
            id="setupOwnerForm"
            className="flex flex-col gap-12"
            autoComplete={DISABLE_AUTOFILL}
          >
            <div className="grid grid-cols-1 gap-y-14 gap-x-8 @lg:grid-cols-2">
              <InputField type="text" name="username" label="Username" />
              <InputField type="text" name="setupKey" label="Setup Key" />
              <InputField type="password" name="password" label="Password" />
              <InputField
                type="password"
                name="passwordConfirm"
                label="Confirm Password"
              />
            </div>
            <div className="flex w-full flex-row justify-between gap-4">
              <Button
                iconRight={faArrowLeft}
                label="Back"
                onClick={navigateBack}
              />
              <Button
                type="submit"
                color="primary"
                label="Submit"
                iconRight={faArrowRight}
                loading={isSubmitting}
              />
            </div>
          </Form>
        )}
      </Formik>
    </div>
  );
};

export default CoreSetupNew;
function setToken(token: string, socket: any) {
  throw new Error('Function not implemented.');
}