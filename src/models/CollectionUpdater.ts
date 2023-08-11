import {useAPIStore} from "../store/store";

export class CollectionUpdater{
    private instance = useAPIStore(state=>state.collections)
}
