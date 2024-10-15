FROM node:21 as build

WORKDIR /app

ENV NODE_ENV production

COPY package*.json /app/
RUN npm ci --legacy-peer-deps

COPY . ./
RUN npm run build

FROM nginx

WORKDIR /usr/share/nginx/html

COPY --from=build /app/out/ ./
# Replace the default server configuration, but leave the overall nginx
# configuration
COPY ./docker/server.nginx.conf /etc/nginx/conf.d/default.conf
