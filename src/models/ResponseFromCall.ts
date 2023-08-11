export type ResponseFromCall = {
    status: string,
    body: string,
    headers: Map,
    cookies: Map,
    duration: TimeMeasures
}

export interface Map {
    [key: string]: string;
}

export type TimeMeasures = {
    response_duration: String,
    duration: String
}
