import {Tabs, TabsContent, TabsList, TabsTrigger} from "../bareComponents/Tabs";

export const CollectionViewer = ()=>{
    return (
    <Tabs defaultValue="authorization" className="w-[400px] import-tabs">
        <TabsList className="query-param-list">
            <TabsTrigger value="authorization">Authorisierung</TabsTrigger>
            <TabsTrigger value="pre-request-script">Pre-request Script</TabsTrigger>
            <TabsTrigger value="tests">Tests</TabsTrigger>
            <TabsTrigger value="variables">Variablen</TabsTrigger>
        </TabsList>
        <TabsContent value="authorization"><div/></TabsContent>
        <TabsContent value="pre-request-script">Change your password here.</TabsContent>
        <TabsContent value="tests"><div/></TabsContent>
        <TabsContent value="variables">Change your password here.</TabsContent>
    </Tabs>
    )
}
