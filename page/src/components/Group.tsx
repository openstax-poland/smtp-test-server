import { Group as GroupData } from '~/src/data'

interface Props {
    group: GroupData
}

export default function Group({ group }: Props) {
    return <div className="group">
        <span className="name">{group.name}</span> ({group.members.length} members)
    </div>
}
