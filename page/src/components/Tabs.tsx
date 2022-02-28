// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

import * as React from 'react'

import './Tabs.css'

interface Props<T> {
    tabs: Tab<T>[]
    render: (index: number, tab: T) => React.ReactNode
    selected?: number
}

interface Tab<T> {
    title: string
    data: T
}

export default function Tabs<T>({ tabs, render, selected: defaultSelected = 0 }: Props<T>) {
    const [selected, setSelected] = React.useState(defaultSelected)

    return <div className="tabs">
        <div className="tab-row">
            {tabs.map((tab, index) => <Tab
                key={index}
                title={tab.title}
                index={index}
                onSelect={setSelected}
                selected={selected === index}
                />
            )}
        </div>

        {tabs.map((tab, index) => <div className="tab" data-selected={selected === index}>
            {render(index, tab.data)}
        </div>)}
    </div>
}

interface TabProps {
    title: string
    index: number
    onSelect: (tab: number) => void
    selected: boolean
}

function Tab({ title, index, onSelect, selected }: TabProps) {
    const onClick = React.useCallback(() => onSelect(index), [onSelect, index])

    return <div className="tab-button" data-selected={selected} onClick={onClick}>
        {title}
    </div>
}
