import { describe, it, expect } from 'vitest';
import { bindFaceplateTags } from './autoBind';

describe('TSK-305: Function Block Drag-and-Drop Auto-Binding', () => {
  it('should auto-bind FB_Motor_VFD properties to canonical external tags', () => {
    const binding = bindFaceplateTags('plant1.slurry', 'P101', 'FB_Motor_VFD');

    expect(binding.canonicalTagPrefix).toBe('external.plant1.slurry.P101');
    expect(binding.tagMap['running']).toBe('external.plant1.slurry.P101.bRunning');
    expect(binding.tagMap['actualSpeed']).toBe('external.plant1.slurry.P101.rSpeedFeedback');
    expect(binding.tagMap['speedSetpoint']).toBe('external.plant1.slurry.P101.rSpeedSetpoint');
    expect(binding.tagMap['tripped']).toBe('external.plant1.slurry.P101.bTripped');

    // Directional access checks
    expect(binding.writeAccess['running']).toBe(false);
    expect(binding.writeAccess['speedSetpoint']).toBe(true);
    expect(binding.writeAccess['startCommand']).toBe(true);

    // Write payload generator
    const payload = binding.createWritePayload('speedSetpoint', 1200);
    expect(payload).toEqual({
      tag: 'external.plant1.slurry.P101.rSpeedSetpoint',
      value: 1200,
    });
  });

  it('should prevent writing to read-only properties', () => {
    const binding = bindFaceplateTags('plant1.slurry', 'P101', 'FB_Motor_VFD');
    expect(() => {
      binding.createWritePayload('running', true);
    }).toThrowError(/READ_ONLY/);
  });

  it('should resolve tag path back to faceplate key', () => {
    const binding = bindFaceplateTags('plant1.water', 'XV101', 'FB_Digital_Valve');
    expect(binding.canonicalTagPrefix).toBe('external.plant1.water.XV101');
    expect(binding.resolveTagKey('external.plant1.water.XV101.bOpenLimit')).toBe('openLimit');
    expect(binding.resolveTagKey('external.plant1.water.XV101.bClosedLimit')).toBe('closedLimit');
  });

  it('should auto-bind FB_PID_Controller parameters', () => {
    const binding = bindFaceplateTags('reactor01', 'TIC101', 'FB_PID_Controller');
    expect(binding.tagMap['pv']).toBe('external.reactor01.TIC101.rPV');
    expect(binding.tagMap['sp']).toBe('external.reactor01.TIC101.rSP');
    expect(binding.tagMap['cv']).toBe('external.reactor01.TIC101.rCV');
    expect(binding.tagMap['kp']).toBe('external.reactor01.TIC101.rKp');
  });
});
