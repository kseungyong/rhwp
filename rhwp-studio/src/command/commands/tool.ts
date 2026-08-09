import type { CommandDef } from '../types';
import { OptionsDialog } from '../../ui/options-dialog';
import { isDesktopRuntime } from '@/command/desktop-file-system';
import { openDiagnosticsLogFolder } from '@/core/diagnostics-log-folder';

export const toolCommands: CommandDef[] = [
  {
    id: 'tool:options',
    label: '환경 설정',
    execute(services) {
      const dlg = new OptionsDialog(services.eventBus);
      dlg.show();
    },
  },
  {
    // 문제 보고용. 직원이 로그 파일을 찾아 첨부할 수 있어야 로그를 남기는 의미가 있다.
    id: 'tool:open-log-folder',
    label: '진단 로그 폴더 열기',
    canExecute: () => isDesktopRuntime(),
    execute() {
      void openDiagnosticsLogFolder()
        .then((result) => {
          if (result.revealed) return;
          const copied = result.copied ? '\n\n경로를 클립보드에 복사했습니다.' : '';
          alert(`진단 로그 폴더:\n${result.path}${copied}`);
        })
        .catch((error: unknown) => {
          const message = error instanceof Error ? error.message : String(error);
          alert(`진단 로그 폴더를 열지 못했습니다:\n${message}`);
        });
    },
  },
];
