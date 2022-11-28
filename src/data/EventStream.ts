import { addInstance, deleteInstance, updateInstance } from 'data/InstanceList';
import { LodestoneContext } from 'data/LodestoneContext';
import { useQueryClient } from '@tanstack/react-query';
import { useCallback, useContext, useEffect, useMemo } from 'react';
import { InstanceState } from 'bindings/InstanceState';
import { ClientEvent } from 'bindings/ClientEvent';
import { match, otherwise, partial } from 'variant';
import { NotificationContext } from './NotificationContext';
import { EventQuery } from 'bindings/EventQuery';
import axios from 'axios';

/**
 * does not return anything, call this for the side effect of subscribing to the event stream
 * information will be available in the query cache of the respective query cache
 */
export const useEventStream = () => {
  const queryClient = useQueryClient();
  const { dispatch, ongoingDispatch } = useContext(NotificationContext);
  const { isReady, token, address, port, apiVersion } =
    useContext(LodestoneContext);

  const eventQuery: EventQuery = useMemo(
    () => ({
      bearer_token: token,
      event_levels: null,
      event_types: null,
      instance_event_types: null,
      user_event_types: null,
      event_instance_ids: null,
    }),
    [token]
  );

  const updateInstanceState = useCallback(
    (uuid: string, state: InstanceState) => {
      updateInstance(uuid, queryClient, (oldInfo) => {
        return { ...oldInfo, state };
      });
    },
    [queryClient]
  );
  const updateInstancePlayerCount = useCallback(
    (uuid: string, increment: number) => {
      updateInstance(uuid, queryClient, (oldInfo) => {
        return {
          ...oldInfo,
          player_count: oldInfo.player_count
            ? oldInfo.player_count + increment
            : oldInfo.player_count,
        };
      });
    },
    [queryClient]
  );

  const handleEvent = useCallback(
    (event: ClientEvent, fresh: boolean) => {
      const { event_inner, snowflake_str } = event;

      match(event_inner, {
        InstanceEvent: ({
          instance_event_inner: event_inner,
          instance_uuid: uuid,
          instance_name: name,
        }) =>
          match(event_inner, {
            InstanceStarting: () => {
              if (fresh) updateInstanceState(uuid, 'Starting');
              dispatch({
                title: `Starting instance ${name}`,
                event,
                type: 'add',
              });
            },
            InstanceStarted: () => {
              if (fresh) updateInstanceState(uuid, 'Running');
              dispatch({
                title: `Instance ${name} started`,
                event,
                type: 'add',
              });
            },
            InstanceStopping: () => {
              if (fresh) updateInstanceState(uuid, 'Stopping');
              dispatch({
                title: `Stopping instance ${name}`,
                event,
                type: 'add',
              });
            },
            InstanceStopped: () => {
              if (fresh) updateInstanceState(uuid, 'Stopped');
              dispatch({
                title: `Instance ${name} stopped`,
                event,
                type: 'add',
              });
            },
            InstanceWarning: () => {
              alert(
                "Warning: An instance has encountered a warning. This shouldn't happen, please report this to the developers."
              );
              dispatch({
                title: `Instance ${name} encountered a warning`,
                event,
                type: 'add',
              });
            },
            InstanceError: () => {
              if (fresh) updateInstanceState(uuid, 'Error');
              dispatch({
                title: `Instance ${name} encountered an error`,
                event,
                type: 'add',
              });
            },
            InstanceInput: ({ message }) => {
              console.log(`Got input on ${name}: ${message}`);
            },
            InstanceOutput: ({ message }) => {
              console.log(`Got output on ${name}: ${message}`);
            },
            SystemMessage: ({ message }) => {
              console.log(`Got system message on ${name}: ${message}`);
            },
            PlayerChange: ({ player_list, players_joined, players_left }) => {
              // updateInstancePlayerCount(uuid, player_list.length);
              console.log(`Got player change on ${name}: ${player_list}`);
              console.log(`${players_joined} joined ${name}`);
              console.log(`${players_left} left ${name}`);
              updateInstancePlayerCount(uuid, players_joined.length);
              updateInstancePlayerCount(uuid, -players_left.length);
              const title = `${
                players_joined.length > 0
                  ? `${players_joined.join(', ')} Joined ${name}`
                  : ''
              }
              ${
                players_left.length > 0 && players_joined.length > 0
                  ? ' and '
                  : ''
              }
              ${
                players_left.length > 0
                  ? `${players_left.join(', ')} Left ${name}`
                  : ''
              }`;
              dispatch({
                title,
                event,
                type: 'add',
              });
            },
            PlayerMessage: ({ player, player_message }) => {
              console.log(`${player} said ${player_message} on ${name}`);
              dispatch({
                title: `${player} said ${player_message} on ${name}`,
                event,
                type: 'add',
              });
            },
          }),
        UserEvent: ({ user_id: uid, user_event_inner: event_inner }) =>
          match(event_inner, {
            UserCreated: () => {
              console.log(`User ${uid} created`);
              dispatch({
                title: `User ${uid} created`,
                event,
                type: 'add',
              });
            },
            UserDeleted: () => {
              console.log(`User ${uid} deleted`);
              dispatch({
                title: `User ${uid} deleted`,
                event,
                type: 'add',
              });
            },
            UserLoggedIn: () => {
              console.log(`User ${uid} logged in`);
              dispatch({
                title: `User ${uid} logged in`,
                event,
                type: 'add',
              });
            },
            UserLoggedOut: () => {
              console.log(`User ${uid} logged out`);
              dispatch({
                title: `User ${uid} logged out`,
                event,
                type: 'add',
              });
            },
          }),
        MacroEvent: ({
          instance_uuid: uuid,
          macro_uuid: macro_id,
          macro_event_inner: event_inner,
        }) =>
          match(event_inner, {
            MacroStarted: () => {
              console.log(`Macro ${macro_id} started on ${uuid}`);
              dispatch({
                title: `Macro ${macro_id} started on ${uuid}`,
                event,
                type: 'add',
              });
            },
            MacroStopped: () => {
              console.log(`Macro ${macro_id} stopped on ${uuid}`);
              dispatch({
                title: `Macro ${macro_id} stopped on ${uuid}`,
                event,
                type: 'add',
              });
            },
            MacroErrored: ({ error_msg }) => {
              console.log(`Macro ${macro_id} errored on ${uuid}: ${error_msg}`);
              dispatch({
                title: `Macro ${macro_id} errored on ${uuid}: ${error_msg}`,
                event,
                type: 'add',
              });
            },
          }),
        ProgressionEvent: (progressionEvent) => {
          ongoingDispatch({
            event,
            progressionEvent,
            dispatch,
          });
          // check if there's a "value"
          const inner = progressionEvent.progression_event_inner;
          if (fresh) {
            match(
              inner,
              otherwise(
                {
                  ProgressionEnd: ({ inner, message }) => {
                    if (!inner) return;
                    match(
                      inner,
                      otherwise(
                        {
                          InstanceCreation: (instance_info) =>
                            addInstance(instance_info, queryClient),
                          InstanceDelete: ({ instance_uuid: uuid }) =>
                            deleteInstance(uuid, queryClient),
                        },
                        // eslint-disable-next-line @typescript-eslint/no-empty-function
                        (_) => {}
                      )
                    );
                  },
                  ProgressionStart: ({ inner }) => {
                    if (!inner) return;
                    match(
                      inner,
                      otherwise(
                        {
                          // InstanceDelete: ({ instance_uuid: uuid }) =>
                          //   deleteInstance(uuid, queryClient),
                        },
                        // eslint-disable-next-line @typescript-eslint/no-empty-function
                        (_) => {}
                      )
                    );
                  },
                },
                // eslint-disable-next-line @typescript-eslint/no-empty-function
                (_) => {}
              )
            );
          }
        },
        FSEvent: ({ operation, target }) => {
          // console.log(`FS ${operation} on ${target.path}`);
          // match(target, {
          //   File: ({ path }) => {
          //     dispatch({
          //       title: `FS ${operation} on ${path}`,
          //       event,
          //       type: 'add',
          //     });
          //   },
          //   Directory: ({ path }) => {
          //     dispatch({
          //       title: `FS ${operation} on ${path}`,
          //       event,
          //       type: 'add',
          //     });
          //   },
          // });
        },
      });
    },
    [
      dispatch,
      ongoingDispatch,
      queryClient,
      updateInstancePlayerCount,
      updateInstanceState,
    ]
  );

  useEffect(() => {
    if (!isReady) return;
    if (!token) return;

    dispatch({
      type: 'clear',
    });

    const bufferAddress = `/events/all/buffer?filter=${JSON.stringify(
      eventQuery
    )}`;

    axios.get<Array<ClientEvent>>(bufferAddress).then((response) => {
      response.data.forEach((event) => {
        handleEvent(event, false);
      });
    });

    const wsAddress = `ws://${address}:${
      port ?? 16662
    }/api/${apiVersion}/events/all/stream?filter=${JSON.stringify(eventQuery)}`;

    const websocket = new WebSocket(wsAddress);

    // if the websocket because error, we should try to reconnect
    websocket.onerror = (event) => {
      console.error('websocket error', event);
      // alert('Disconnected from server, please refresh the page to reconnect');
    };

    websocket.onmessage = (messageEvent) => {
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const event: ClientEvent = JSON.parse(messageEvent.data);
      handleEvent(event, true);
    };

    return () => {
      websocket.close();
    };
  }, [
    address,
    apiVersion,
    eventQuery,
    handleEvent,
    isReady,
    port,
    queryClient,
    token,
  ]);
};
