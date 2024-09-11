export type EnvironmentType = {
    id: string,
    type: string,
    initialValue: string,
    currentValue: string
}


export type EnvironmentWrapper = {
    name: string,
    envVars: EnvironmentType[]
}