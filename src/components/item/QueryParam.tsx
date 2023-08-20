import {Tabs, TabsContent, TabsList, TabsTrigger} from "../bareComponents/Tabs";
import {ParamTable} from "./QueryParamTable";
import {HeaderTable} from "./HeaderTable";
import {BodyDecider} from "./BodyDecider";
import {AuthorizationContent} from "../collection_authorization/AuthorizationContent";
import {CollectionAuthorization} from "../collections/CollectionAuthorization";

export const QueryParam = () => {


    return <Tabs defaultValue="params" className="request-details-section selector text-white">
        <TabsList className="query-param-list">
            <TabsTrigger value="params" className="">Params</TabsTrigger>
            <TabsTrigger value="authorization">Authorization</TabsTrigger>
            <TabsTrigger value="headers">Headers</TabsTrigger>
            <TabsTrigger value="body">Body</TabsTrigger>
            <TabsTrigger value="prerequest">Pre-request Script</TabsTrigger>
            <TabsTrigger value="tests">Tests</TabsTrigger>
            <TabsTrigger value="settings">Settings</TabsTrigger>
        </TabsList>
        <TabsContent value="params"><ParamTable/></TabsContent>
        <TabsContent value="authorization"><CollectionAuthorization/></TabsContent>
        <TabsContent value="headers"><HeaderTable/></TabsContent>
        <TabsContent value="body"><BodyDecider/></TabsContent>
        <TabsContent value="prerequest">Pre-request Script</TabsContent>
        <TabsContent value="tests">Change your password here.</TabsContent>
        <TabsContent value="settings">Change your password here.</TabsContent>
    </Tabs>
}
