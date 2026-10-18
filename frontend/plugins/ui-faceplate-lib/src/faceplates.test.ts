import { describe, it, expect } from 'vitest';
import React from 'react';
import { renderToString } from 'react-dom/server';
import { MotorFaceplate } from './components/MotorFaceplate';
import { ValveFaceplate } from './components/ValveFaceplate';
import { AnalogTransmitterFaceplate } from './components/AnalogTransmitterFaceplate';
import { PidControllerFaceplate } from './components/PidControllerFaceplate';

function normalizeHtml(html: string): string {
  return html.replace(/<!-- -->/g, '');
}

describe('TSK-304: 3-Tier Visual Faceplate Component Library', () => {
  describe('MotorFaceplate (FB_Motor_VFD)', () => {
    it('should render Tier 1 P&ID Symbol with equipment tag and speed', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(MotorFaceplate, {
          tag: 'P-101A',
          label: 'Slurry Feed Pump A',
          running: true,
          tripped: false,
          actualSpeed: 1450,
          speedSetpoint: 1500,
          tier: 1,
        })
      ));
      expect(html).toContain('P-101A');
      expect(html).toContain('1450 RPM');
      expect(html).toContain('faceplate-tier-1');
    });

    it('should render Tier 2 Summary Card with live readouts and controls', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(MotorFaceplate, {
          tag: 'P-101A',
          label: 'Slurry Feed Pump A',
          running: true,
          tripped: false,
          actualSpeed: 1450,
          speedSetpoint: 1500,
          currentAmps: 18.5,
          torquePercent: 78,
          tier: 2,
        })
      ));
      expect(html).toContain('faceplate-tier-2');
      expect(html).toContain('Slurry Feed Pump A');
      expect(html).toContain('18.5 A');
      expect(html).toContain('78 %');
      expect(html).toContain('START');
      expect(html).toContain('STOP');
    });

    it('should render Tier 3 Diagnostic Faceplate with interlock checklist', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(MotorFaceplate, {
          tag: 'P-101A',
          label: 'Slurry Feed Pump A',
          running: false,
          tripped: true,
          actualSpeed: 0,
          speedSetpoint: 1500,
          tier: 3,
        })
      ));
      expect(html).toContain('faceplate-tier-3');
      expect(html).toContain('Safety Interlock Matrix (ISO 13849-1)');
      expect(html).toContain('Maint. Lock');
      expect(html).toContain('FB_Motor_VFD');
    });
  });

  describe('ValveFaceplate (FB_Digital_Valve)', () => {
    it('should render Tier 1 with valve triangles and status', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(ValveFaceplate, {
          tag: 'XV-101',
          label: 'Slurry Inlet Isolation Valve',
          openLimit: true,
          closedLimit: false,
          transitFault: false,
          tier: 1,
        })
      ));
      expect(html).toContain('valve-tier-1');
      expect(html).toContain('XV-101');
      expect(html).toContain('OPEN');
    });

    it('should render Tier 2 summary card with position indicator', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(ValveFaceplate, {
          tag: 'XV-101',
          label: 'Slurry Inlet Valve',
          openLimit: false,
          closedLimit: true,
          transitFault: false,
          positionPercent: 0,
          tier: 2,
        })
      ));
      expect(html).toContain('valve-tier-2');
      expect(html).toContain('FULLY CLOSED');
      expect(html).toContain('0%');
    });
  });

  describe('AnalogTransmitterFaceplate (FB_Analog_Transmitter)', () => {
    it('should render Tier 1 circular transmitter bubble with PV', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(AnalogTransmitterFaceplate, {
          tag: 'PT-101',
          label: 'Discharge Header Pressure',
          pv: 6.84,
          unit: 'bar',
          tier: 1,
        })
      ));
      expect(html).toContain('analog-tier-1');
      expect(html).toContain('PT-101');
      expect(html).toContain('6.8');
      expect(html).toContain('bar');
    });

    it('should render Tier 2 summary card with limit bounds', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(AnalogTransmitterFaceplate, {
          tag: 'PT-101',
          label: 'Discharge Header Pressure',
          pv: 6.84,
          unit: 'bar',
          minRange: 0,
          maxRange: 16,
          highLimit: 12,
          highHighLimit: 14,
          tier: 2,
        })
      ));
      expect(html).toContain('analog-tier-2');
      expect(html).toContain('6.84');
      expect(html).toContain('16 bar');
    });
  });

  describe('PidControllerFaceplate (FB_PID_Controller)', () => {
    it('should render Tier 2 card with PV, SP, CV and Auto/Manual mode', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(PidControllerFaceplate, {
          tag: 'TIC-101',
          label: 'Primary Reactor Temp',
          pv: 82.5,
          sp: 80.0,
          cv: 45.2,
          unit: '°C',
          tier: 2,
        })
      ));
      expect(html).toContain('pid-tier-2');
      expect(html).toContain('TIC-101');
      expect(html).toContain('82.5');
      expect(html).toContain('80.0');
      expect(html).toContain('45.2 %');
    });

    it('should render Tier 3 diagnostic tuning drawer with Kp, Tn, Tv fields', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(PidControllerFaceplate, {
          tag: 'TIC-101',
          label: 'Primary Reactor Temp',
          pv: 82.5,
          sp: 80.0,
          cv: 45.2,
          unit: '°C',
          kp: 1.5,
          tn: 20.0,
          tv: 3.0,
          tier: 3,
        })
      ));
      expect(html).toContain('pid-tier-3');
      expect(html).toContain('Loop Tuning Parameters');
      expect(html).toContain('Gain (Kp)');
      expect(html).toContain('Integral (Tn s)');
    });
  });
});
