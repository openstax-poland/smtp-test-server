// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

import { Group as GroupData } from '~/src/data'

interface Props {
    group: GroupData
}

export default function Group({ group }: Props) {
    return <div className="group">
        <span className="name">{group.name}</span> ({group.members.length} members)
    </div>
}
