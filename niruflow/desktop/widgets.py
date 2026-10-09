"""Reusable, deliberately neutral Qt table helpers."""

def make_table(headers):
    from PySide6.QtWidgets import QTableWidget, QHeaderView, QAbstractItemView
    t=QTableWidget(0,len(headers))
    t.setHorizontalHeaderLabels(headers)
    t.horizontalHeader().setSectionResizeMode(QHeaderView.ResizeToContents)
    t.horizontalHeader().setStretchLastSection(True)
    t.setSelectionBehavior(QAbstractItemView.SelectRows)
    t.setSelectionMode(QAbstractItemView.ExtendedSelection)
    t.setEditTriggers(QAbstractItemView.NoEditTriggers)
    t.setAlternatingRowColors(True)
    t.setSortingEnabled(True)
    t.verticalHeader().hide()
    return t

def populate_table(table, rows, enable_sort=True):
    from PySide6.QtCore import Qt
    from PySide6.QtWidgets import QTableWidgetItem
    table.setSortingEnabled(False)
    table.setUpdatesEnabled(False)
    try:
        table.setRowCount(len(rows))
        for i,row in enumerate(rows):
            for j,value in enumerate(row):
                item=QTableWidgetItem(str(value))
                if isinstance(value,(int,float)):
                    item.setData(Qt.EditRole,value)
                table.setItem(i,j,item)
    finally:
        table.setUpdatesEnabled(True)
        table.setSortingEnabled(enable_sort)
