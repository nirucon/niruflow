"""NIRUFLOW desktop shell. Views use the same independent core as the CLI."""
import json
import sys
from datetime import datetime
from .localization import tr, apply_locale, format_datetime, REGIONS, DATE_PATTERNS, TIME_PATTERNS
from pathlib import Path

from ..core.config import load, save
from ..core.version import VERSION
from ..core.backup import export_config, import_config
from ..core.scheduler import status as scheduler_status, install as scheduler_install, set_enabled as scheduler_set_enabled
from ..core.planning import preview_rules
from ..core.locations import locations, location_paths, add_location, profile
from ..core.store import history
from ..core.safety import is_nextcloud
from ..modules.cleanup import scan, quarantine, restore
from ..modules.projects import analyze
from ..modules.insights import project_insights
from ..modules.docker import inspect, container_action, container_remove, logs, mounts
from ..modules.forgotten import find_old_files
from ..modules.compose import discover
from ..modules.storage import largest, duplicates, storage_quarantine, storage_delete, file_category
from ..modules.docker_cleanup import preview as image_cleanup_preview, apply as image_cleanup_apply
from ..core.formatting import human_size
from .themes import theme_names, stylesheet
from .widgets import make_table, populate_table


def launch():
    from PySide6.QtCore import Qt, QObject, QThread, Signal, QTimer
    from PySide6.QtWidgets import (QApplication, QMainWindow, QWidget, QVBoxLayout,
        QHBoxLayout, QLabel, QPushButton, QComboBox, QStackedWidget, QTableWidget,
        QTableWidgetItem, QCheckBox, QSpinBox, QMessageBox as QtMessageBox, QPlainTextEdit,
        QHeaderView, QAbstractItemView, QFrame, QLineEdit, QSplitter, QFileDialog,
        QProgressBar, QInputDialog, QTabWidget, QDateEdit, QTimeEdit, QDialog, QDialogButtonBox, QFormLayout)

    class NeutralMessageBox(QtMessageBox):
        @staticmethod
        def question(parent, title, message, buttons=None, default=None):
            dialog=QMessageBox(parent)
            dialog.setIcon(QMessageBox.NoIcon)
            dialog.setWindowTitle(title)
            dialog.setText(message)
            dialog.setStandardButtons(buttons if buttons is not None else QMessageBox.Yes|QMessageBox.No)
            dialog.setDefaultButton(default if default is not None else QMessageBox.No)
            return dialog.exec()

        @staticmethod
        def information(parent,title,message):
            dialog=QMessageBox(parent)
            dialog.setIcon(QMessageBox.NoIcon)
            dialog.setWindowTitle(title)
            dialog.setText(message)
            dialog.setStandardButtons(QMessageBox.Ok)
            return dialog.exec()

        @staticmethod
        def warning(parent,title,message):
            return NeutralMessageBox.information(parent,title,message)

    QMessageBox=NeutralMessageBox

    class Worker(QObject):
        finished = Signal(object)
        failed = Signal(str)
        progress = Signal(object)
        cancelled = Signal()
        def __init__(self, task, cancellable=False):
            super().__init__()
            self.task = task
            self.cancellable = cancellable
            import threading
            self.cancel_event = threading.Event()
        def run(self):
            try:
                result = self.task(self.cancel_event.is_set, self.progress.emit) if self.cancellable else self.task()
                if self.cancellable and self.cancel_event.is_set():
                    self.cancelled.emit()
                else:
                    self.finished.emit(result)
            except Exception as exc:
                self.failed.emit(str(exc))

    def label(text, muted=False):
        item=QLabel(text)
        if muted:
            item.setObjectName('muted')
        return item

    def button(text, callback):
        b=QPushButton(text)
        b.clicked.connect(callback)
        return b

    def table(headers):
        return make_table(headers)

    def fill(t, rows, enable_sort=True):
        populate_table(t, rows, enable_sort)

    class Window(QMainWindow):
        def __init__(self):
            super().__init__()
            self.cfg=load()
            self.jobs=[]
            self.storage_job=None
            self.candidates=[]
            self.image_cleanup_plan=None
            self.setWindowTitle(f'NIRUFLOW {VERSION}')
            self.resize(1180,760)
            host=QWidget()
            layout=QVBoxLayout(host)
            layout.setContentsMargins(12,12,12,8)
            header=QHBoxLayout()
            title=label('NIRUFLOW  /  LOCAL AUTOMATION')
            title.setObjectName('brand')
            header.addWidget(title)
            header.addStretch()
            header.addWidget(label('Theme',True))
            self.theme=QComboBox()
            self.theme.addItems(theme_names())
            self.theme.setCurrentText(self.cfg.get('theme','Niru Noir'))
            self.theme.currentTextChanged.connect(self.change_theme)
            header.addWidget(self.theme)
            header.addWidget(button('Close', self.close))
            layout.addLayout(header)
            splitter=QSplitter(Qt.Horizontal)
            self.sidebar=QFrame()
            self.sidebar.setObjectName('sidebar')
            side=QVBoxLayout(self.sidebar)
            side.setContentsMargins(7,12,7,8)
            side.setSpacing(3)
            self.stack=QStackedWidget()
            self.nav=[]
            self.pages={}
            for name in ('Overview','Automations','File Cleanup','Projects','Docker','Storage','Media Library','History','Settings'):
                nav=QPushButton(name)
                nav.setObjectName('nav')
                nav.setCheckable(True)
                index=len(self.nav)
                nav.clicked.connect(lambda _checked=False, idx=index:self.navigate(idx))
                self.nav.append(nav)
                side.addWidget(nav)
                page=QWidget()
                page_layout=QVBoxLayout(page)
                page_layout.setContentsMargins(15,12,15,12)
                page_layout.setSpacing(11)
                heading=label(name)
                heading.setObjectName('pageTitle')
                page_layout.addWidget(heading)
                self.pages[name]=page_layout
                self.stack.addWidget(page)
            side.addStretch()
            side.addWidget(label(f'{VERSION}  ·  Ing Leif Nicklas Rudolfsson',True))
            splitter.addWidget(self.sidebar)
            splitter.addWidget(self.stack)
            splitter.setSizes([192,980])
            splitter.setCollapsible(0,False)
            layout.addWidget(splitter)
            self.setCentralWidget(host)
            self.statusBar().showMessage('Ready · destructive actions require confirmation')
            self.build_overview()
            self.build_automations()
            self.build_cleanup()
            self.build_projects()
            self.build_docker()
            self.build_storage()
            self.build_media()
            self.build_history()
            self.build_settings()
            apply_locale(QApplication.instance(),self.cfg.get('locale','sv_SE'))
            self.change_theme(self.theme.currentText())
            self.navigate(0)
        def closeEvent(self,event):
            if any(thread.isRunning() for thread in self.jobs):
                QMessageBox.information(self,'Analysis in progress','Wait until the current analysis finishes before closing the window.')
                event.ignore()
                return
            event.accept()
        def change_theme(self,theme):
            QApplication.instance().setStyleSheet(stylesheet(theme))
            self.cfg['theme']=theme
            save(self.cfg)
        def navigate(self,index):
            self.stack.setCurrentIndex(index)
            for i,nav in enumerate(self.nav):
                nav.setChecked(i==index)
            if index==0:self.refresh_overview()
            if index==1:self.refresh_automations()
            if index==7:self.refresh_history()
            if index==4 and not getattr(self,'docker_data',None):self.refresh_docker()
        def run_task(self,task,on_success, *, cancellable=False, progress=None, on_end=None):
            # Thread ownership retained until quit/finished, no blocking UI work.
            thread=QThread(self)
            worker=Worker(task,cancellable)
            if cancellable:
                self.storage_job=worker
            worker.moveToThread(thread)
            thread.started.connect(worker.run)
            worker.finished.connect(on_success)
            if progress is not None:
                worker.progress.connect(progress)
            worker.failed.connect(lambda message:(self.statusBar().showMessage('Error: '+message),QMessageBox.warning(self,'Analysis failed',message)))
            worker.finished.connect(lambda _result:self.statusBar().showMessage('Analysis complete'))
            worker.finished.connect(thread.quit)
            worker.failed.connect(thread.quit)
            worker.cancelled.connect(thread.quit)
            worker.cancelled.connect(lambda:self.statusBar().showMessage('Scan cancelled · no files changed'))
            if cancellable:
                worker.cancelled.connect(lambda:self.storage_info.setText('Scan cancelled · no files changed'))
            if on_end is not None:
                thread.finished.connect(on_end)
            thread.finished.connect(worker.deleteLater)
            thread.finished.connect(thread.deleteLater)
            thread._niruflow_worker=worker
            self.jobs.append(thread)
            thread.finished.connect(lambda:self.jobs.remove(thread) if thread in self.jobs else None)
            thread.start()
            self.statusBar().showMessage('Analysis running in the background…')
        def build_overview(self):
            v=self.pages['Overview']
            v.addWidget(label('Local maintenance and automation · No cloud service required',True))
            self.overview_status=label('')
            v.addWidget(self.overview_status)
            self.overview_disk=label('')
            v.addWidget(self.overview_disk)
            self.overview_history=table(['Time','Rule','Action','File'])
            self.overview_history.setMaximumHeight(320)
            v.addWidget(label('Recent activity'))
            v.addWidget(self.overview_history)
            self.overview_empty=label('',True)
            v.addWidget(self.overview_empty)
            v.addStretch(1)
            v.addWidget(button('Refresh overview',self.refresh_overview))
        def refresh_overview(self):
            import shutil
            rules=self.cfg.get('rules',[])
            enabled=sum(bool(x.get('enabled')) for x in rules)
            rows=history(10)
            self.overview_status.setText(f'Automations: {enabled}/{len(rules)} enabled    ·    Recent actions: {len(rows)}    ·    Nextcloud: manual review')
            try:
                disk=shutil.disk_usage(Path.home())
                self.overview_disk.setText(f'Home filesystem    ·    Free: {human_size(disk.free)}    ·    Used: {human_size(disk.used)}    ·    Capacity: {human_size(disk.total)}')
            except OSError:
                self.overview_disk.setText('Filesystem usage unavailable')
            fill(self.overview_history,[(self.display_datetime(x[1]),x[2],x[5],Path(x[3]).name) for x in rows])
            self.overview_empty.setText('No recent actions. Run a preview in File Cleanup, Storage or Docker to get started.' if not rows else '')
        def build_automations(self):
            v=self.pages['Automations']
            v.addWidget(label('Rules run independently through CLI or the optional background scheduler.',True))
            self.rules_table=table(['Rule','Directory','Age','Enabled','Background'])
            v.addWidget(self.rules_table)
            v.addWidget(label('Synced Nextcloud paths are excluded from unattended cleanup.',True))
            controls=QHBoxLayout()
            controls.addWidget(button('Enable / Disable',self.toggle_rule_ui))
            controls.addWidget(button('Edit rule',self.edit_rule_ui))
            controls.addWidget(button('Preview',self.preview_rule_ui))
            controls.addWidget(button('Run now (quarantine)',self.run_rule_ui))
            v.addLayout(controls)
        def refresh_automations(self):
            fill(self.rules_table,[(r['id'],r['path'],f"{r.get('hours',24)} h",'Yes' if r.get('enabled') else 'No',
                    'Manual' if is_nextcloud(r['path']) else 'Allowed') for r in self.cfg['rules']])
        def selected_rule(self):
            rows=self.rules_table.selectedItems()
            if not rows:return None
            key=self.rules_table.item(rows[0].row(),0).text()
            return next((r for r in self.cfg['rules'] if r['id']==key),None)
        def toggle_rule_ui(self):
            rule=self.selected_rule()
            if not rule:return
            rule['enabled']=not rule.get('enabled',False)
            save(self.cfg);self.refresh_automations()
        def edit_rule_ui(self):
            rule=self.selected_rule()
            if not rule:return
            hours=self.choose_age(int(rule.get('hours',24)))
            if hours is None:return
            rule['hours']=hours
            if QMessageBox.question(self,'Directory','Change the monitored directory?')==QMessageBox.Yes:
                directory=QFileDialog.getExistingDirectory(self,'Select directory',str(Path(rule['path']).expanduser()))
                if directory:rule['path']=directory
            save(self.cfg);self.refresh_automations()
        def preview_rule_ui(self):
            rule=self.selected_rule()
            if rule:self.run_task(lambda:scan(rule),lambda rows:QMessageBox.information(self,'Dry run',f'{len(rows)} candidate files. No changes made.'))
        def run_rule_ui(self):
            rule=self.selected_rule()
            if not rule:return
            if is_nextcloud(rule['path']):
                QMessageBox.warning(self,'Synced directory','Automatic actions in Nextcloud are blocked. Use manual File Cleanup review.')
                return
            if not rule.get('enabled'):
                QMessageBox.information(self,'Disabled','Enable this rule first.')
                return
            if QMessageBox.question(self,'Confirm quarantine',f'Move eligible files from {rule["path"]} into quarantine?')!=QMessageBox.Yes:return
            self.run_task(lambda:quarantine(rule,scan(rule)),lambda rows:(self.refresh_automations(),QMessageBox.information(self,'Completed',f'{len(rows)} files moved to quarantine.')))
        def set_age_selection(self,combo,hours):
            index=combo.findData(hours)
            if index<0:
                combo.addItem(f'{hours} hours (custom)',hours)
                index=combo.findData(hours)
            combo.setCurrentIndex(index)
        def choose_age(self,current=24):
            dialog=QDialog(self)
            dialog.setWindowTitle('File age')
            form=QFormLayout(dialog)
            amount=QSpinBox();amount.setRange(1,3650)
            units=QComboBox()
            for name,multiplier in [('Hours',1),('Days',24),('Weeks',168)]:units.addItem(name,multiplier)
            if current%168==0:units.setCurrentIndex(2);amount.setValue(current//168)
            elif current%24==0:units.setCurrentIndex(1);amount.setValue(current//24)
            else:amount.setValue(current)
            form.addRow('Amount',amount);form.addRow('Unit',units)
            buttons=QDialogButtonBox(QDialogButtonBox.Ok|QDialogButtonBox.Cancel)
            buttons.accepted.connect(dialog.accept);buttons.rejected.connect(dialog.reject)
            form.addRow(buttons)
            return amount.value()*units.currentData() if dialog.exec()==QDialog.Accepted else None
        def build_cleanup(self):
            v=self.pages['File Cleanup']
            row=QHBoxLayout()
            self.rulebox=QComboBox()
            self.rulebox.addItems([r['id'] for r in self.cfg['rules']])
            self.rulebox.currentIndexChanged.connect(self.show_rule)
            row.addWidget(self.rulebox,2)
            self.enabled=QCheckBox('Enabled')
            row.addWidget(self.enabled)
            self.age=QComboBox()
            self.age.setEditable(False)
            for hours, caption in [(1,'1 hour'),(6,'6 hours'),(12,'12 hours'),(24,'24 hours'),(48,'2 days'),(72,'3 days'),(168,'7 days'),(336,'14 days'),(720,'30 days'),(2160,'90 days'),(4320,'180 days'),(8760,'365 days')]:
                self.age.addItem(caption,hours)
            row.addWidget(self.age)
            row.addWidget(button('Choose directory',self.choose_rule_path))
            row.addWidget(button('Save rule',self.save_rule))
            v.addLayout(row)
            self.cleanup_info=label('',True)
            v.addWidget(self.cleanup_info)
            self.cleanup_table=table(['File','Size','Modified','Full path'])
            v.addWidget(self.cleanup_table)
            controls=QHBoxLayout()
            controls.addWidget(button('Analyze',self.preview))
            controls.addWidget(button('Select all',self.cleanup_table.selectAll))
            controls.addWidget(button('Move selected to quarantine',self.apply_cleanup))
            v.addLayout(controls)
            v.addWidget(label('Subdirectories are not traversed. Protected names are excluded.',True))
            self.show_rule()
        def rule(self):
            return self.cfg['rules'][self.rulebox.currentIndex()]
        def show_rule(self):
            if self.rulebox.currentIndex()<0:return
            r=self.rule()
            self.enabled.setChecked(bool(r.get('enabled')))
            self.set_age_selection(self.age,int(r.get('hours',24)))
            self.cleanup_table.setRowCount(0)
            self.candidates=[]
            self.cleanup_info.setText('Directory: '+r['path']+'  ·  Run a scan to show candidates')
        def choose_rule_path(self):
            directory=QFileDialog.getExistingDirectory(self,'Choose cleanup directory',str(Path(self.rule()['path']).expanduser()))
            if directory:
                self.rule()['path']=directory
                save(self.cfg)
                self.show_rule()
        def save_rule(self):
            r=self.rule()
            r['enabled']=self.enabled.isChecked()
            r['hours']=self.age.currentData()
            save(self.cfg)
            self.statusBar().showMessage('Rule saved')
        def preview(self):
            r=dict(self.rule())
            r['hours']=self.age.currentData()
            self.run_task(lambda:scan(r),self.preview_done)
        def preview_done(self,results):
            self.candidates=results
            fill(self.cleanup_table,[(Path(x['path']).name,human_size(x['bytes']),self.display_datetime(datetime.fromtimestamp(x['mtime'])),x['path']) for x in results])
            total=sum(x['bytes'] for x in results)
            self.cleanup_info.setText(f'{len(results)} candidates · {total/1048576:.1f} MiB · preview only')
            self.statusBar().showMessage('Analysis complete; no files changed')
        def apply_cleanup(self):
            if not self.rule().get('enabled'):
                QMessageBox.information(self,'Rule is disabled','Enable and save the rule first.')
                return
            chosen={item.row() for item in self.cleanup_table.selectedItems()}
            paths={self.cleanup_table.item(row,3).text() for row in chosen}
            candidates=[x for x in self.candidates if x['path'] in paths]
            if not candidates:
                QMessageBox.information(self,'Nothing selected','Select files in the table first.')
                return
            synced=is_nextcloud(self.rule()['path'])
            msg=f'Move {len(candidates)} selected files to local quarantine?'
            if synced:msg+='\n\nVARNING: Nextcloud synkroniserar bortflyttningen som radering till andra enheter.'
            if QMessageBox.question(self,'Confirm action',msg)!=QMessageBox.Yes:return
            try:
                moved=quarantine(self.rule(),candidates,allow_synced=synced)
                self.statusBar().showMessage(f'{len(moved)} files moved to quarantine')
                self.preview()
            except Exception as exc:
                QMessageBox.warning(self,'Action failed',str(exc))
        def build_projects(self):
            v=self.pages['Projects']
            v.addWidget(label('Read-only inventory. Patch chains and unknown artifacts are protected.',True))
            row=QHBoxLayout()
            row.addWidget(button('Analyze project directories',self.refresh_projects))
            self.project_filter=QLineEdit()
            self.project_filter.setPlaceholderText('Filter projects or filenames…')
            self.project_filter.textChanged.connect(self.filter_projects)
            row.addWidget(self.project_filter)
            v.addLayout(row)
            self.projects_table=table(['Project','Artifact','Type','Version','From → To','Assessment'])
            v.addWidget(self.projects_table)
            v.addWidget(label('“Review” does not mean safe to delete. Project inventory is read-only.',True))
            self.project_rows=[]
            self.projects_status=label('No project scan yet. Analysis is read-only.',True)
            v.addWidget(self.projects_status)
        def refresh_projects(self):
            paths=location_paths(self.cfg,'projects')
            self.run_task(lambda:[entry for path in paths for entry in analyze(path)],self.projects_done)
        def projects_done(self,rows):
            self.project_rows=rows
            self.filter_projects()
            insight=project_insights(rows)
            self.projects_status.setText(f"{insight['total']} artifacts · {insight['protected']} protected · {insight['review']} to review · read-only" if rows else 'No project artifacts found in configured directories.')
            self.statusBar().showMessage(f'{len(rows)} project artifacts inventoried')
        def filter_projects(self):
            query=self.project_filter.text().casefold()
            rows=[x for x in self.project_rows if query in (x['project']+' '+x['name']).casefold()]
            fill(self.projects_table,[(x['project'],x['name'],x['kind'],x['version'],
                (x['from']+' → '+x['to']) if x['from'] else '',x['status']) for x in rows])
        def build_docker(self):
            v=self.pages['Docker']
            v.addWidget(label('Docker Engine · explicit confirmation for actions; no unattended deletion.',True))
            v.addWidget(button('Refresh Docker',self.refresh_docker))
            self.docker_summary=label('Refresh Docker to inspect the current context.')
            v.addWidget(self.docker_summary)
            self.docker_containers=table(['Name','Image','Status','Health','Size'])
            self.docker_containers.setSelectionMode(QAbstractItemView.SingleSelection)
            self.docker_containers.itemSelectionChanged.connect(self.update_container_selection)
            container_page=QWidget(); container_layout=QVBoxLayout(container_page)
            search_row=QHBoxLayout()
            self.container_search=QLineEdit();self.container_search.setPlaceholderText('Search containers...')
            self.container_search.textChanged.connect(self.filter_containers)
            search_row.addWidget(self.container_search)
            self.container_filter=QComboBox();self.container_filter.addItems(['All statuses','Running','Stopped'])
            self.container_filter.currentIndexChanged.connect(self.filter_containers)
            search_row.addWidget(self.container_filter)
            container_layout.addLayout(search_row)
            self.selected_container_label=label('Selected: none',True)
            container_layout.addWidget(self.selected_container_label)
            container_layout.addWidget(self.docker_containers)
            actions=QHBoxLayout()
            for action,caption in [('start','Start'),('stop','Stop'),('restart','Restart'),('pause','Pause'),('unpause','Resume')]:
                actions.addWidget(button(caption,lambda _=False,a=action:self.docker_action_ui(a)))
            actions.addWidget(button('Logs',self.docker_logs_ui))
            actions.addWidget(button('Mounts',self.docker_mounts_ui))
            actions.addWidget(button('Remove stopped container',self.docker_remove_ui))
            container_layout.addLayout(actions)
            self.docker_images=table(['Repository','Tag','ID','Size'])
            image_page=QWidget();image_layout=QVBoxLayout(image_page)
            self.image_search=QLineEdit();self.image_search.setPlaceholderText('Search images...')
            self.image_search.textChanged.connect(self.filter_images)
            image_layout.addWidget(self.image_search)
            image_layout.addWidget(self.docker_images)
            cleanup_row=QHBoxLayout()
            cleanup_row.addWidget(label('Image cleanup · minimum age (days)'))
            self.image_age=QComboBox()
            for days in (1,3,7,10,14,30,60,90,180,365):self.image_age.addItem(f'{days} days',days)
            self.image_age.setCurrentIndex(self.image_age.findData(7))
            cleanup_row.addWidget(self.image_age)
            self.include_unused=QCheckBox('Include unused tagged images')
            self.include_unused.setChecked(True)
            cleanup_row.addWidget(self.include_unused)
            cleanup_row.addWidget(button('Preview cleanup',self.preview_images_ui))
            cleanup_row.addWidget(button('Delete selected images',self.delete_images_ui))
            cleanup_page=QWidget();cleanup_layout=QVBoxLayout(cleanup_page)
            cleanup_layout.addLayout(cleanup_row)
            self.image_cleanup_info=label('Preview includes tagged images not referenced by containers. No volumes are deleted.',True)
            cleanup_layout.addWidget(self.image_cleanup_info)
            self.image_cleanup_table=table(['Image ID','Tags','Created','Size'])
            self.image_cleanup_table.setMinimumHeight(280)
            self.image_cleanup_table.setSortingEnabled(True)
            cleanup_layout.addWidget(self.image_cleanup_table,1)
            cleanup_layout.addWidget(button('Select all candidates',self.image_cleanup_table.selectAll))
            self.docker_disk=QPlainTextEdit()
            self.docker_disk.setReadOnly(True)
            self.docker_disk.setMaximumHeight(125)
            cleanup_layout.addWidget(label('Docker disk usage'))
            cleanup_layout.addWidget(self.docker_disk)
            compose_page=QWidget();compose_layout=QVBoxLayout(compose_page)
            compose_layout.addWidget(label('Compose projects on disk (read-only inventory)'))
            compose_layout.addWidget(button('Discover Compose files',self.refresh_compose))
            self.compose_table=table(['Project','Compose file','Search root'])
            compose_layout.addWidget(self.compose_table)
            self.docker_tabs=QTabWidget()
            for title,page in [('Containers',container_page),('Images',image_page),('Compose',compose_page),('Cleanup',cleanup_page)]:self.docker_tabs.addTab(page,title)
            v.addWidget(self.docker_tabs,1)
        def update_container_selection(self):
            entry=self.selected_container()
            self.selected_container_label.setText('Selected: '+entry.get('Names','')+'  ·  '+entry.get('Status','') if entry else 'Selected: none')
        def filter_containers(self,*_):
            query=self.container_search.text().casefold()
            status=self.container_filter.currentIndex()
            for i in range(self.docker_containers.rowCount()):
                name=' '.join(self.docker_containers.item(i,j).text() for j in (0,1,2)).casefold()
                state=self.docker_containers.item(i,2).text().casefold()
                self.docker_containers.setRowHidden(i,query not in name or (status==1 and not state.startswith('up')) or (status==2 and state.startswith('up')))
        def filter_images(self,*_):
            query=self.image_search.text().casefold()
            for i in range(self.docker_images.rowCount()):
                self.docker_images.setRowHidden(i,query not in ' '.join(self.docker_images.item(i,j).text() for j in (0,1)).casefold())
        def preview_images_ui(self):
            self.image_cleanup_plan=None
            self.image_cleanup_table.setRowCount(0)
            days=self.image_age.currentData();include=self.include_unused.isChecked()
            self.image_cleanup_info.setText('Scanning Docker images and container references...')
            self.run_task(lambda:image_cleanup_preview(days,include),self.image_preview_done)
        def image_preview_done(self,plan):
            self.image_cleanup_plan=plan
            self.image_cleanup_table.setSortingEnabled(False)
            fill(self.image_cleanup_table,[(x['id'],', '.join(x['tags']) or '<dangling>',
                 x['created'][:10],human_size(x['bytes'])) for x in plan['candidates']])
            self.image_cleanup_table.setSortingEnabled(True)
            skipped=plan['excluded']
            self.image_cleanup_info.setText(
                f"{plan['count']} candidates · {human_size(plan['apparent_bytes'])} summed image sizes (NOT recoverable space) · "
                f"{skipped['in_use']} referenced by containers · {skipped['too_new']} newer than cutoff · "
                f"{skipped['tagged_opt_out']} tagged excluded · {skipped['invalid']} unknown · context: {plan['context']}")
        def delete_images_ui(self):
            plan=self.image_cleanup_plan
            if not plan:
                QMessageBox.information(self,'Preview required','Run Preview cleanup first.')
                return
            if plan['age_days']!=self.image_age.currentData() or plan['include_unused']!=self.include_unused.isChecked():
                QMessageBox.warning(self,'Policy changed','Preview again after changing policy.')
                return
            rows={x.row() for x in self.image_cleanup_table.selectedItems()}
            ids=[self.image_cleanup_table.item(i,0).text() for i in sorted(rows)]
            if not ids:return
            answer,ok=QInputDialog.getText(self,'Delete Docker images',
                f'Delete {len(ids)} selected images? This may remove rollback images.\n'
                'No containers or volumes are removed.\nType DELETE IMAGES:')
            if not ok or answer!='DELETE IMAGES':return
            self.docker_disk_before=self.docker_disk.toPlainText()
            self.run_task(lambda:image_cleanup_apply(plan,ids,answer),self.images_deleted)
        def images_deleted(self,result):
            self.image_cleanup_plan=None
            self.image_cleanup_table.setRowCount(0)
            self.image_cleanup_info.setText(f"Removed {len(result['removed'])} images ({result.get('removed_tags',0)} tags); errors: {len(result['errors'])}. Refresh and preview again.")
            self.refresh_docker()
            if result['errors']:
                from collections import Counter
                categories=Counter('Container reference' if 'container' in x['error'].lower() else
                                   'Tag/repository conflict' if 'multiple repositories' in x['error'].lower() else
                                   'Changed or unavailable' for x in result['errors'])
                dialog=QMessageBox(self)
                dialog.setIcon(QMessageBox.NoIcon)
                dialog.setWindowTitle('Docker cleanup results')
                dialog.setText(f"Removed {len(result['removed'])} images and {result.get('removed_tags',0)} tags.\n"
                               f"{len(result['errors'])} images could not be removed.")
                dialog.setInformativeText('\n'.join(f'{k}: {v}' for k,v in categories.items()))
                dialog.setDetailedText('\n\n'.join(f"{x['id']}: {x['error']}" for x in result['errors']))
                dialog.exec()
        def refresh_compose(self):
            roots=location_paths(self.cfg,'docker')
            self.run_task(lambda:discover(roots),self.compose_done)
        def compose_done(self,rows):
            fill(self.compose_table,[(x['project'],x['path'],x['root']) for x in rows])
            self.statusBar().showMessage(f'{len(rows)} Compose files discovered')
        def refresh_docker(self):
            self.run_task(inspect,self.docker_done)
        def docker_done(self,data):
            self.docker_data=data
            summary=data['summary']
            self.docker_summary.setText(f"Context: {data.get('context','?')} · {summary['containers']} containers · {summary['running']} running · {summary['unhealthy']} unhealthy · {summary['images']} images")
            fill(self.docker_containers,[(x.get('Names',''),x.get('Image',''),x.get('Status',''),
                'Unhealthy' if 'unhealthy' in x.get('Status','').lower() else '',x.get('Size','')) for x in data['containers']])
            fill(self.docker_images,[(x.get('Repository',''),x.get('Tag',''),x.get('ID',''),x.get('Size','')) for x in data['images']])
            self.docker_disk.setPlainText(data['disk'])
            self.filter_containers();self.filter_images()
            self.statusBar().showMessage('Docker status updated · no changes made')
        def selected_container(self):
            rows=self.docker_containers.selectedItems()
            if not rows:return None
            name=self.docker_containers.item(rows[0].row(),0).text()
            data=getattr(self,'docker_data',{})
            return next((x for x in data.get('containers',[]) if x.get('Names')==name),None)
        def docker_action_ui(self,action):
            entry=self.selected_container()
            if not entry:
                QMessageBox.information(self,'Select container','Select a container first.')
                return
            ident=entry.get('ID','')
            if not ident:
                QMessageBox.warning(self,'Missing ID','Cannot perform the action without a container ID.')
                return
            if QMessageBox.question(self,'Confirm Docker action',
                    f"{action.upper()} on {entry.get('Names')}?\n\nContainer-ID: {ident[:16]}\nThis action may affect service availability.")!=QMessageBox.Yes:return
            self.run_task(lambda:container_action(ident,action),lambda _result:self.refresh_docker())
        def docker_remove_ui(self):
            entry=self.selected_container()
            if not entry:
                QMessageBox.information(self,'Select container','Select a stopped container first.')
                return
            if str(entry.get('State','')).lower() not in ('exited','created','dead'):
                QMessageBox.warning(self,'Container is not stopped','Only stopped containers can be removed. Running containers must be stopped separately.')
                return
            ident=entry.get('ID','')
            context=getattr(self,'docker_data',{}).get('context','')
            name=entry.get('Names','')
            answer,ok=QInputDialog.getText(self,'Remove container permanently',
                f'Remove stopped container {name}?\nID: {ident[:16]}\nContext: {context}\n\n'
                'The container will be permanently removed. Volumes are NOT removed, but changes inside its writable layer will be lost.\n'
                'Compose may recreate the container later.\n\nType REMOVE CONTAINER:')
            if not ok or answer!='REMOVE CONTAINER':return
            self.run_task(lambda:container_remove(ident,context,'REMOVE CONTAINER'),
                lambda _result:self.refresh_docker())
        def docker_logs_ui(self):
            entry=self.selected_container()
            if not entry:return
            self.run_task(lambda:logs(entry.get('ID','')),
                lambda content:self.show_docker_text('Containerloggar',content))
        def docker_mounts_ui(self):
            entry=self.selected_container()
            if not entry:return
            self.run_task(lambda:mounts(entry.get('ID','')),
                lambda data:self.show_docker_text('Persistenta datakopplingar',json.dumps(data,indent=2,ensure_ascii=False)))
        def show_docker_text(self,title,content):
            dialog=QMessageBox(self)
            dialog.setWindowTitle(title)
            dialog.setText(title)
            dialog.setDetailedText(str(content)[-25000:] or 'Ingen information')
            dialog.exec()
        def build_storage(self):
            v=self.pages['Storage']
            v.addWidget(label('Storage Explorer · inspect before moving files to quarantine. Synced files need extra confirmation.',True))
            row=QHBoxLayout()
            self.storage_scope=QComboBox()
            self.storage_scope.addItems(['Home','Nextcloud','Choose directory…'])
            row.addWidget(self.storage_scope)
            self.storage_months=QComboBox()
            for months in (1,3,6,12,18,24,36,60,120):
                self.storage_months.addItem(f'{months} months',months)
            self.storage_months.setCurrentIndex(self.storage_months.findData(12))
            row.addWidget(self.storage_months)
            row.addWidget(button('Old files',self.refresh_storage))
            row.addWidget(button('Largest files',self.refresh_largest))
            row.addWidget(button('Duplicates',self.refresh_duplicates))
            v.addLayout(row)
            filters=QHBoxLayout()
            self.storage_search=QLineEdit();self.storage_search.setPlaceholderText('Filter by name or path…')
            self.storage_search.textChanged.connect(self.filter_storage)
            filters.addWidget(self.storage_search,3)
            self.storage_type=QComboBox();self.storage_type.addItems(['All types','Archives','Video','Audio','Images','Documents','Other'])
            self.storage_type.currentIndexChanged.connect(self.filter_storage)
            filters.addWidget(self.storage_type)
            self.storage_sort=QComboBox();self.storage_sort.addItems(['Largest first','Smallest first','Newest first','Oldest first','Name A–Z'])
            self.storage_sort.currentIndexChanged.connect(self.filter_storage)
            filters.addWidget(self.storage_sort)
            v.addLayout(filters)
            self.storage_info=label('Run a scan to see results. No files are changed.',True)
            self.storage_cancel=button('Cancel scan',self.cancel_storage_scan)
            self.storage_cancel.setEnabled(False)
            v.addWidget(self.storage_cancel)
            v.addWidget(self.storage_info)
            self.storage_table=table(['File','Size','Modified','Type','Directory'])
            self.storage_table.setSelectionMode(QAbstractItemView.ExtendedSelection)
            self.storage_table.setSortingEnabled(False)
            self.storage_table.setContextMenuPolicy(Qt.CustomContextMenu)
            self.storage_table.customContextMenuRequested.connect(self.storage_context_menu)
            v.addWidget(self.storage_table,1)
            actions=QHBoxLayout()
            actions.addWidget(button('Open directory',self.storage_open_directory))
            actions.addWidget(button('Select all visible',self.storage_select_visible))
            actions.addWidget(button('Move selected to quarantine',self.storage_move_selected))
            actions.addWidget(button('Delete permanently',self.storage_delete_selected))
            v.addLayout(actions)
            self.storage_rows=[]
            self.storage_display=[]
            self.storage_mode='files'
        def storage_context_menu(self,pos):
            from PySide6.QtWidgets import QMenu
            menu=QMenu(self)
            menu.addAction('Open directory',self.storage_open_directory)
            menu.addAction('Move selected to quarantine',self.storage_move_selected)
            menu.addAction('Delete permanently',self.storage_delete_selected)
            menu.exec(self.storage_table.viewport().mapToGlobal(pos))
        def storage_select_visible(self):
            from PySide6.QtCore import QItemSelectionModel
            self.storage_table.clearSelection()
            for row in range(self.storage_table.rowCount()):
                if not self.storage_table.isRowHidden(row):
                    idx=self.storage_table.model().index(row,0)
                    self.storage_table.selectionModel().select(idx,QItemSelectionModel.Select|QItemSelectionModel.Rows)
        def storage_selected(self):
            indexes={item.row() for item in self.storage_table.selectedItems()}
            return [self.storage_display[row] for row in sorted(indexes) if row<len(self.storage_display)]
        def storage_open_directory(self):
            from PySide6.QtGui import QDesktopServices
            from PySide6.QtCore import QUrl
            rows=self.storage_selected()
            if rows:QDesktopServices.openUrl(QUrl.fromLocalFile(str(Path(rows[0]['path']).parent)))
        def filter_storage(self,*_):
            query=self.storage_search.text().casefold()
            kind=self.storage_type.currentText()
            rows=[x for x in self.storage_rows if query in x['path'].casefold() and (kind=='All types' or x['category']==kind)]
            mode=self.storage_sort.currentIndex()
            if mode==0:rows.sort(key=lambda x:x['bytes'],reverse=True)
            elif mode==1:rows.sort(key=lambda x:x['bytes'])
            elif mode==2:rows.sort(key=lambda x:x['mtime'],reverse=True)
            elif mode==3:rows.sort(key=lambda x:x['mtime'])
            else:rows.sort(key=lambda x:Path(x['path']).name.casefold())
            self.storage_display=rows
            # Populate without Qt's lexical re-sort of human-readable sizes.
            self.storage_table.setSortingEnabled(False)
            fill(self.storage_table,[(Path(x['path']).name,human_size(x['bytes']),
                 self.display_datetime(datetime.fromtimestamp(x['mtime'])),x['category'],str(Path(x['path']).parent)) for x in rows], enable_sort=False)
            self.storage_info.setText(f'Showing {len(rows)} of {len(self.storage_rows)} scanned files · {human_size(sum(x["bytes"] for x in rows))} displayed total · {self.storage_mode} · no changes')
        def storage_move_selected(self):
            rows=self.storage_selected()
            if not rows:return
            if self.storage_mode=='duplicates':
                QMessageBox.warning(self,'Duplicates','Review individual files before removing duplicates. Duplicate-group view is read-only.')
                return
            synced=any(is_nextcloud(x['path']) for x in rows)
            total=sum(x['bytes'] for x in rows)
            message=f'Move {len(rows)} files ({human_size(total)}) to quarantine?\n\nFiles are revalidated before moving.\nQuarantine requires the same filesystem.'
            if synced:message+='\n\nWARNING: Nextcloud will synchronize the removal to other devices.'
            if QMessageBox.question(self,'Confirm quarantine',message)!=QMessageBox.Yes:return
            if synced:
                answer,ok=QInputDialog.getText(self,'Confirm Nextcloud change','Type SYNC DELETE to confirm removal from synchronized directories:')
                if not ok or answer!='SYNC DELETE':return
            self.run_task(lambda:storage_quarantine(rows,allow_synced=synced),self.storage_moved)
        def storage_delete_selected(self):
            rows=self.storage_selected()
            if not rows:return
            if self.storage_mode=='duplicates':
                QMessageBox.warning(self,'Duplicates','Review duplicate copies individually using another scan before deletion.')
                return
            synced=any(is_nextcloud(x['path']) for x in rows)
            total=sum(x['bytes'] for x in rows)
            message=(f'PERMANENTLY delete {len(rows)} selected files ({human_size(total)})?\n\n'
                     'This cannot be undone locally. Files will be checked again before deletion.')
            if synced:
                message+='\n\nNextcloud will synchronize these deletions to the server and other devices.'
            if QMessageBox.question(self,'Confirm permanent deletion',message,QMessageBox.Yes|QMessageBox.No,QMessageBox.No)!=QMessageBox.Yes:return
            phrase='DELETE SYNCED' if synced else 'DELETE'
            answer,ok=QInputDialog.getText(self,'Final confirmation',f'Type {phrase} to permanently delete the selected files:')
            if not ok or answer!=phrase:return
            self.run_task(lambda:storage_delete(rows,allow_synced=synced),self.storage_deleted)
        def storage_deleted(self,result):
            deleted=set(result['deleted'])
            self.storage_rows=[x for x in self.storage_rows if x['path'] not in deleted]
            self.filter_storage()
            QMessageBox.information(self,'Permanent deletion',f'{len(deleted)} files deleted.\n{len(result["skipped"])} skipped.\nSee History for the audit log.')
        def storage_moved(self,result):
            moved=set(result['moved'])
            self.storage_rows=[x for x in self.storage_rows if x['path'] not in moved]
            self.filter_storage()
            QMessageBox.information(self,'Quarantine',f'{len(moved)} files moved to quarantine.\n{len(result["skipped"])} skipped.\nRestore via History.')
        def cancel_storage_scan(self):
            if self.storage_job is not None:
                self.storage_job.cancel_event.set()
                self.storage_info.setText('Cancelling scan safely…')
                self.storage_cancel.setEnabled(False)

        def storage_scan_progress(self,details):
            count,folder=details
            self.storage_info.setText(f'Scanning · {count:,} files examined · {folder} · no changes')

        def storage_scan_finished(self):
            self.storage_job=None
            self.storage_cancel.setEnabled(False)

        def start_storage_scan(self,task,done):
            if self.storage_job is not None:
                self.storage_info.setText('A scan is already running. Cancel it before starting another.')
                return
            self.storage_info.setText('Starting scan… no files are changed')
            self.storage_cancel.setEnabled(True)
            self.run_task(task,done,cancellable=True,progress=self.storage_scan_progress,on_end=self.storage_scan_finished)

        def refresh_storage(self):
            if self.storage_job is not None:return
            index=self.storage_scope.currentIndex()
            if index==0:root=Path.home()
            elif index==1:root=Path.home()/'Nextcloud'
            else:
                directory=QFileDialog.getExistingDirectory(self,'Choose directory',str(Path.home()))
                if not directory:return
                root=Path(directory)
            months=self.storage_months.currentData()
            self.start_storage_scan(lambda cancelled,progress:find_old_files(root,months=months,limit=300,cancelled=cancelled,progress=progress),lambda rows:self.storage_done(rows,'old files'))
        def refresh_largest(self):
            roots=location_paths(self.cfg,'storage')
            self.start_storage_scan(lambda cancelled,progress:largest(roots,limit=500,cancelled=cancelled,progress=progress),lambda rows:self.storage_done(rows,'largest files'))
        def refresh_duplicates(self):
            roots=location_paths(self.cfg,'storage')
            self.start_storage_scan(lambda cancelled,progress:duplicates(roots,limit=50,cancelled=cancelled,progress=progress),self.duplicates_done)
        def duplicates_done(self,groups):
            # Display each copy independently. No automatic deduplication.
            rows=[]
            for group in groups:
                for path in group['paths']:
                    try:
                        st=Path(path).stat()
                        rows.append({'path':path,'bytes':st.st_size,'mtime':st.st_mtime,'category':file_category(path)})
                    except OSError:continue
            self.storage_done(rows,'duplicate copies (review carefully)')
            self.storage_mode='duplicates'
            self.filter_storage()
        def storage_done(self,rows,mode='files'):
            self.storage_mode=mode
            self.storage_rows=[{'path':x['path'],'bytes':x['bytes'],
               'mtime':x.get('mtime') or (datetime.fromisoformat(x['modified']).timestamp() if x.get('modified') else 0),
               'category':file_category(x['path'])} for x in rows]
            self.filter_storage()
        def build_media(self):
            from ..modules.media import default_roots
            v=self.pages['Media Library']
            v.addWidget(label('Read-only media inventory. Scans run in the background; no files are modified.',True))
            row=QHBoxLayout()
            self.media_roots=QLineEdit('; '.join(self.cfg.get('media_roots',default_roots())))
            self.media_roots.setPlaceholderText('Directories separated by semicolons')
            row.addWidget(self.media_roots,1)
            row.addWidget(button('Add directory',self.add_media_root))
            row.addWidget(button('Scan media',self.refresh_media))
            v.addLayout(row)
            self.media_summary=label('No scan yet')
            v.addWidget(self.media_summary)
            self.media_table=table(['Type','Files','Size','Extensions'])
            v.addWidget(self.media_table)
        def add_media_root(self):
            path=QFileDialog.getExistingDirectory(self,'Choose media directory',str(Path.home()))
            if path:
                roots=[x.strip() for x in self.media_roots.text().split(';') if x.strip()]
                if path not in roots:roots.append(path)
                self.media_roots.setText('; '.join(roots))
        def refresh_media(self):
            from ..modules.media import inventory
            roots=[x.strip() for x in self.media_roots.text().split(';') if x.strip()]
            self.cfg['media_roots']=roots;save(self.cfg)
            self.media_summary.setText('Scanning...')
            self.run_task(lambda:inventory(roots),self.media_done)
        def media_done(self,data):
            fill(self.media_table,[(kind,stats['count'],human_size(stats['bytes']),', '.join(f'{ext}: {n}' for ext,n in sorted(stats['extensions'].items(),key=lambda x:-x[1])[:7])) for kind,stats in data['types'].items()])
            self.media_summary.setText(f"{data['total']} files · {human_size(data['bytes'])} · {data['errors']} inaccessible directories/files · {data['duration_s']} s")
        def build_history(self):
            v=self.pages['History']
            v.addWidget(label('Quarantined files can be restored if the original path is available.',True))
            self.history_table=table(['ID','Time','Rule','Original','Status','Size'])
            v.addWidget(self.history_table)
            row=QHBoxLayout()
            row.addWidget(button('Refresh',self.refresh_history))
            row.addWidget(button('Restore selected action',self.restore_selected))
            v.addLayout(row)
        def refresh_history(self):
            self.history_rows=history(300)
            fill(self.history_table,[(x[0],self.display_datetime(x[1]),x[2],x[3],x[5],x[6]) for x in self.history_rows])
        def restore_selected(self):
            selected=self.history_table.selectedItems()
            if not selected:return
            action_id=int(self.history_table.item(selected[0].row(),0).text())
            action=next((x for x in self.history_rows if x[0]==action_id),None)
            if not action or action[5]!='quarantined':
                QMessageBox.information(self,'Cannot restore','Select a quarantine entry.')
                return
            if QMessageBox.question(self,'Restore',f'Restore {action[3]}?')!=QMessageBox.Yes:return
            try:
                restore(action_id)
                self.refresh_history()
                self.statusBar().showMessage('File restored')
            except Exception as exc:
                QMessageBox.warning(self,'Restore failed',str(exc))
        def build_settings(self):
            v=self.pages['Settings']
            v.addWidget(label(f'NIRUFLOW {VERSION} · Ing Leif Nicklas Rudolfsson · MIT'))
            v.addWidget(label('Interface language: English · Regional formatting'))
            locale_row=QHBoxLayout()
            self.locale_box=QComboBox();self.locale_box.addItems(REGIONS)
            self.locale_box.setCurrentText(self.cfg.get('locale','sv_SE'))
            self.locale_box.currentTextChanged.connect(self.change_locale)
            locale_row.addWidget(label('Regional format'));locale_row.addWidget(self.locale_box)
            self.date_box=QComboBox();self.date_box.addItems(DATE_PATTERNS)
            self.date_box.setCurrentText(self.cfg.get('date_format','yyyy-MM-dd'))
            self.date_box.currentTextChanged.connect(self.change_date_format)
            locale_row.addWidget(label('Date format'));locale_row.addWidget(self.date_box)
            self.time_box=QComboBox();self.time_box.addItems(TIME_PATTERNS)
            self.time_box.setCurrentText(self.cfg.get('time_format','HH:mm'))
            self.time_box.currentTextChanged.connect(self.change_time_format)
            locale_row.addWidget(label('Time format'));locale_row.addWidget(self.time_box)
            v.addLayout(locale_row)
            self.weekday_box=QCheckBox('Show weekday (e.g. mån, tis, ons for Sweden)')
            self.weekday_box.setChecked(self.cfg.get('show_weekday',True))
            self.weekday_box.toggled.connect(self.change_weekday)
            v.addWidget(self.weekday_box)
            self.date_example=label('',True)
            v.addWidget(self.date_example)
            self.update_date_example()
            v.addWidget(label('Change theme in the upper-right corner.',True))
            v.addWidget(label('Configuration: ~/.config/niruflow/config.json',True))
            v.addWidget(label('History and quarantine: ~/.local/state/niruflow/',True))
            v.addWidget(label('Scheduler: optional systemd user timer.',True))
            v.addWidget(label('Unattended Nextcloud cleanup is blocked.',True))
            v.addWidget(button('Preview automation safety',self.preview_automation_safety))
            v.addWidget(label('Background scheduling · independent of desktop window'))
            self.scheduler_info=label('',True)
            v.addWidget(self.scheduler_info)
            scheduler_actions=QHBoxLayout()
            scheduler_actions.addWidget(button('Refresh status',self.refresh_scheduler))
            scheduler_actions.addWidget(button('Install scheduler',self.install_scheduler_ui))
            scheduler_actions.addWidget(button('Enable',lambda:self.toggle_scheduler_ui(True)))
            scheduler_actions.addWidget(button('Pause',lambda:self.toggle_scheduler_ui(False)))
            v.addLayout(scheduler_actions)
            v.addWidget(label('The scheduler checks enabled rules hourly, even when the desktop is closed. Synced directories are skipped.',True))
            v.addWidget(label('Configuration backup · profiles, rules and preferences; excludes history and quarantine'))
            backup_actions=QHBoxLayout()
            backup_actions.addWidget(button('Export configuration',self.export_backup_ui))
            backup_actions.addWidget(button('Import configuration',self.import_backup_ui))
            v.addLayout(backup_actions)
            self.refresh_scheduler()
            profile_row=QHBoxLayout()
            profile_row.addWidget(label('Profile'))
            self.profile_box=QComboBox()
            self.profile_box.addItems(self.cfg.get('profiles',{}).keys())
            self.profile_box.setCurrentText(self.cfg.get('active_profile','default'))
            self.profile_box.currentTextChanged.connect(self.change_profile)
            profile_row.addWidget(self.profile_box)
            profile_row.addWidget(button('New profile',self.new_profile_ui))
            v.addLayout(profile_row)
            v.addWidget(label('Monitored locations · profile-specific directories'))
            self.locations_table=table(['Name','Type','Directory','Enabled'])
            v.addWidget(self.locations_table)
            actions=QHBoxLayout()
            actions.addWidget(button('Add location',self.add_location_ui))
            actions.addWidget(button('Edit selected',self.edit_location_ui))
            actions.addWidget(button('Enable / Pause',self.toggle_location_ui))
            actions.addWidget(button('Remove selected',self.remove_location_ui))
            v.addLayout(actions)
            self.refresh_locations()
        def preview_automation_safety(self):
            rows=preview_rules(self.cfg)
            lines=[f"{r['name']}: {r['status']} — {r['path']}" for r in rows]
            QMessageBox.information(self,'Automation safety preview',
                '\n'.join(lines) if lines else 'No automation rules configured.')

        def display_datetime(self,value):
            return format_datetime(value, self.cfg.get('locale','sv_SE'),
                self.cfg.get('date_format','yyyy-MM-dd'),
                self.cfg.get('time_format','HH:mm'), self.cfg.get('show_weekday',True))
        def update_date_example(self):
            self.date_example.setText('Example: ' + self.display_datetime(datetime(2026,10,9,22,0)))
        def change_locale(self,value):
            self.cfg['locale']=value;save(self.cfg);apply_locale(QApplication.instance(),value)
            self.update_date_example();self.refresh_displayed_dates()
        def change_date_format(self,value):
            self.cfg['date_format']=value;save(self.cfg)
            self.update_date_example();self.refresh_displayed_dates()
        def change_time_format(self,value):
            self.cfg['time_format']=value;save(self.cfg)
            self.update_date_example();self.refresh_displayed_dates()
        def change_weekday(self,value):
            self.cfg['show_weekday']=value;save(self.cfg)
            self.update_date_example();self.refresh_displayed_dates()
        def refresh_displayed_dates(self):
            if hasattr(self,'history_table'): self.refresh_history()
            if hasattr(self,'storage_table') and hasattr(self,'storage_rows'):
                self.filter_storage()
        def refresh_scheduler(self):
            info=scheduler_status()
            if not info['supported']:
                self.scheduler_info.setText('Not available on this system (the CLI can use another scheduler).')
            else:
                self.scheduler_info.setText(f"Installed: {'Yes' if info['installed'] else 'No'}  ·  Active: {'Yes' if info['active'] else 'No'}  ·  Enabled at login: {'Yes' if info['enabled'] else 'No'}")
        def install_scheduler_ui(self):
            if QMessageBox.question(self,'Install scheduler','Install an hourly systemd user timer? Existing rules will not change.')!=QMessageBox.Yes:return
            try:
                scheduler_install();self.refresh_scheduler()
            except Exception as exc:QMessageBox.warning(self,'Scheduler',str(exc))
        def toggle_scheduler_ui(self,enabled):
            try:
                scheduler_set_enabled(enabled);self.refresh_scheduler()
            except Exception as exc:QMessageBox.warning(self,'Scheduler',str(exc))
        def export_backup_ui(self):
            target,_=QFileDialog.getSaveFileName(self,'Export configuration',str(Path.home()/'niruflow-config-backup.zip'),'ZIP (*.zip)')
            if not target:return
            try:
                export_config(target)
                QMessageBox.information(self,'Backup skapad',f'Konfigurationen exporterades till:\n{target}')
            except Exception as exc:QMessageBox.warning(self,'Export misslyckades',str(exc))
        def import_backup_ui(self):
            source,_=QFileDialog.getOpenFileName(self,'Import configuration',str(Path.home()),'ZIP (*.zip)')
            if not source:return
            if QMessageBox.question(self,'Confirm import','Import replaces profiles, rules and settings. The current configuration is backed up first. Continue?')!=QMessageBox.Yes:return
            try:
                import_config(source)
                QMessageBox.information(self,'Import complete','Configuration imported. Restart NIRUFLOW to load the updated profiles and rules.')
            except Exception as exc:QMessageBox.warning(self,'Import misslyckades',str(exc))
        def change_profile(self,key):
            if key and key in self.cfg.get('profiles',{}):
                self.cfg['active_profile']=key
                save(self.cfg)
                self.refresh_locations()
        def new_profile_ui(self):
            name,ok=QInputDialog.getText(self,'New profile','Profilnamn')
            if not ok or not name.strip():return
            key=name.strip().lower().replace(' ','-')
            if key in self.cfg['profiles']:
                QMessageBox.warning(self,'Already exists','Profile name is already in use')
                return
            self.cfg['profiles'][key]={'name':name.strip(),'locations':[]}
            self.cfg['active_profile']=key
            save(self.cfg)
            self.profile_box.addItem(key)
            self.profile_box.setCurrentText(key)
        def refresh_locations(self):
            fill(self.locations_table,[(x['name'],x['kind'],x['path'],'Yes' if x.get('enabled',True) else 'No') for x in locations(self.cfg)])
        def selected_location(self):
            selected=self.locations_table.selectedItems()
            if not selected:return None
            index=selected[0].row()
            name=self.locations_table.item(index,0).text()
            path=self.locations_table.item(index,2).text()
            return next((x for x in locations(self.cfg) if x['name']==name and x['path']==path),None)
        def add_location_ui(self):
            name,ok=QInputDialog.getText(self,'Ny plats','Name')
            if not ok or not name.strip():return
            kind,ok=QInputDialog.getItem(self,'Location type','Select the purpose of this location',['projects','docker','storage'],0,False)
            if not ok:return
            path=QFileDialog.getExistingDirectory(self,'Choose directory',str(Path.home()))
            if not path:return
            try:
                add_location(self.cfg,name,kind,path)
                save(self.cfg);self.refresh_locations()
            except Exception as exc:QMessageBox.warning(self,'Fel',str(exc))
        def edit_location_ui(self):
            entry=self.selected_location()
            if not entry:return
            path=QFileDialog.getExistingDirectory(self,'Choose directory',str(Path(entry['path']).expanduser()))
            if path:
                entry['path']=path;save(self.cfg);self.refresh_locations()
        def toggle_location_ui(self):
            entry=self.selected_location()
            if entry:
                entry['enabled']=not entry.get('enabled',True)
                save(self.cfg);self.refresh_locations()
        def remove_location_ui(self):
            entry=self.selected_location()
            if not entry:return
            if QMessageBox.question(self,'Remove location','Remove this location from the configuration? No files will be deleted.')!=QMessageBox.Yes:return
            profile(self.cfg)['locations'].remove(entry)
            save(self.cfg);self.refresh_locations()

    app=QApplication.instance() or QApplication(sys.argv)
    window=Window()
    window.show()
    return app.exec()
