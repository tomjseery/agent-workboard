import dayjs from "dayjs";
import relativeTime from "dayjs/plugin/relativeTime";
import utc from "dayjs/plugin/utc";

dayjs.extend(utc);
dayjs.extend(relativeTime);

const local = (value: string) => dayjs.utc(value).local();

export const formatTimestamp = (value: string) => local(value).format("D MMM YYYY HH:mm");

export const formatRelativeTime = (value: string) => local(value).fromNow();

export const formatTimestampWithRelative = (value: string) => `${formatTimestamp(value)} (${formatRelativeTime(value)})`;
