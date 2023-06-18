export type ResponseFromCall = {
    status: string,
    body: string,
    headers: Map<string,string>,
    cookies: Map<string,string>,
    duration: TimeMeasures
}

export type TimeMeasures = {
    response_duration: String,
    duration: String
}
