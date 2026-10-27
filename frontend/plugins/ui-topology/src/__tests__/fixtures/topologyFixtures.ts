/**
 * Fixtures for the topology tests.
 *
 * These were `DEFAULT_STATIONS` / `DEFAULT_CABLES` beside the canvas, wired in as parameter
 * defaults, so the network view asserted a commissioned rack at 192.168.1.10 with Siemens
 * article numbers whether or not anything had been discovered. That is a claim about the plant.
 *
 * They live here so no component can import them. Nothing discovered means nothing drawn.
 */

import type { HardwareStationNode, TopologyCableEdge } from '../../types';

export const STATION_FIXTURE: HardwareStationNode[] = [
  {
    id: 'station-plc-01',
    stationName: 'PLC_RACK_01',
    stationType: 'PLC_RACK',
    rackNumber: 0,
    status: 'RUN',
    ipAddress: '192.168.1.10',
    articleNumber: '6ES7515-2AM02-0AB0',
    position: { x: 80, y: 120 },
    slots: [
      {
        slotNumber: 0,
        name: 'PM 190W 24VDC',
        catalogArticleNo: '6EP1333-4BA00',
        moduleCategory: 'POWER',
        ports: [],
        diagnostics: { runLed: true, errLed: false, maintLed: false, channelBits: [] },
      },
      {
        slotNumber: 1,
        name: 'CPU 1515-2 PN',
        catalogArticleNo: '6ES7515-2AM02-0AB0',
        moduleCategory: 'CPU',
        startAddressI: '%I0.0',
        endAddressI: '%I0.7',
        startAddressQ: '%Q0.0',
        endAddressQ: '%Q0.7',
        ports: [
          { id: 'plc-x1-p1', name: 'X1 P1', protocol: 'PROFINET', connectorType: 'RJ45', linkStatus: 'UP' },
          { id: 'plc-x1-p2', name: 'X1 P2', protocol: 'PROFINET', connectorType: 'RJ45', linkStatus: 'UP' },
        ],
        diagnostics: { runLed: true, errLed: false, maintLed: false, channelBits: [true, true, false, true] },
      },
      {
        slotNumber: 2,
        name: 'DI 16x24VDC HF',
        catalogArticleNo: '6ES7521-1BH00-0AB0',
        moduleCategory: 'DI',
        startAddressI: '%I10.0',
        endAddressI: '%I11.7',
        ports: [],
        diagnostics: { runLed: true, errLed: false, maintLed: false, channelBits: [true, false, true, false, true] },
      },
      {
        slotNumber: 3,
        name: 'DQ 16x24VDC HF',
        catalogArticleNo: '6ES7522-1BH01-0AB0',
        moduleCategory: 'DQ',
        startAddressQ: '%Q10.0',
        endAddressQ: '%Q11.7',
        ports: [],
        diagnostics: { runLed: true, errLed: false, maintLed: false, channelBits: [false, true, true, false] },
      },
    ],
    ports: [
      { id: 'plc-x1-p1', name: 'X1 P1', protocol: 'PROFINET', connectorType: 'RJ45', linkStatus: 'UP' },
      { id: 'plc-x1-p2', name: 'X1 P2', protocol: 'PROFINET', connectorType: 'RJ45', linkStatus: 'UP' },
    ],
  },
  {
    id: 'station-sw-01',
    stationName: 'SCALANCE_XC208',
    stationType: 'SWITCH',
    rackNumber: 0,
    status: 'RUN',
    ipAddress: '192.168.1.2',
    articleNumber: '6GK5208-0BA00-2AC2',
    position: { x: 440, y: 120 },
    slots: [],
    ports: [
      { id: 'sw-p1', name: 'Port 1', protocol: 'PROFINET', connectorType: 'RJ45', linkStatus: 'UP' },
      { id: 'sw-p2', name: 'Port 2', protocol: 'PROFINET', connectorType: 'RJ45', linkStatus: 'UP' },
      { id: 'sw-p3', name: 'Port 3', protocol: 'PROFINET', connectorType: 'RJ45', linkStatus: 'UP' },
      { id: 'sw-p4', name: 'Port 4', protocol: 'SAFETY_PROFISAFE', connectorType: 'RJ45', linkStatus: 'UP' },
    ],
  },
  {
    id: 'station-et200-01',
    stationName: 'ET200SP_ISLAND',
    stationType: 'REMOTE_IO',
    rackNumber: 0,
    status: 'RUN',
    ipAddress: '192.168.1.11',
    articleNumber: '6ES7155-6AU01-0BN0',
    position: { x: 800, y: 60 },
    slots: [
      {
        slotNumber: 1,
        name: 'IM 155-6 PN HF',
        catalogArticleNo: '6ES7155-6AU01-0BN0',
        moduleCategory: 'COMM',
        ports: [],
        diagnostics: { runLed: true, errLed: false, maintLed: false, channelBits: [] },
      },
      {
        slotNumber: 2,
        name: 'AI 4xU/I 2-wire HF',
        catalogArticleNo: '6ES7134-6HD00-0BA1',
        moduleCategory: 'AI',
        startAddressI: '%IW20',
        endAddressI: '%IW26',
        ports: [],
        diagnostics: { runLed: true, errLed: false, maintLed: false, channelBits: [true, true] },
      },
    ],
    ports: [
      { id: 'et-p1', name: 'X1 P1', protocol: 'PROFINET', connectorType: 'RJ45', linkStatus: 'UP' },
      { id: 'et-p2', name: 'X1 P2', protocol: 'PROFINET', connectorType: 'RJ45', linkStatus: 'UP' },
    ],
  },
  {
    id: 'station-vfd-01',
    stationName: 'SINAMICS_G120_PUMP',
    stationType: 'VFD_DRIVE',
    rackNumber: 0,
    status: 'RUN',
    ipAddress: '192.168.1.12',
    articleNumber: '6SL3210-1PE21-8UL0',
    position: { x: 800, y: 280 },
    slots: [],
    ports: [
      { id: 'vfd-p1', name: 'X1 P1', protocol: 'PROFINET', connectorType: 'RJ45', linkStatus: 'UP' },
      { id: 'vfd-p2', name: 'STO', protocol: 'SAFETY_PROFISAFE', connectorType: 'RJ45', linkStatus: 'UP' },
    ],
  },
];

export const CABLE_FIXTURE: TopologyCableEdge[] = [
  {
    id: 'cable-plc-sw',
    sourceStationId: 'station-plc-01',
    sourcePortId: 'plc-x1-p1',
    targetStationId: 'station-sw-01',
    targetPortId: 'sw-p1',
    protocol: 'PROFINET',
    cableLengthMeters: 12.4,
    nominalMatch: true,
    health: { status: 'OPTIMAL', latencyMs: 0.8, packetLossPct: 0.0, retriesPerSec: 0 },
  },
  {
    id: 'cable-sw-et',
    sourceStationId: 'station-sw-01',
    sourcePortId: 'sw-p2',
    targetStationId: 'station-et200-01',
    targetPortId: 'et-p1',
    protocol: 'PROFINET',
    cableLengthMeters: 4.2,
    nominalMatch: true,
    health: { status: 'OPTIMAL', latencyMs: 0.5, packetLossPct: 0.0, retriesPerSec: 0 },
  },
  {
    id: 'cable-sw-vfd',
    sourceStationId: 'station-sw-01',
    sourcePortId: 'sw-p3',
    targetStationId: 'station-vfd-01',
    targetPortId: 'vfd-p1',
    protocol: 'PROFINET',
    cableLengthMeters: 8.5,
    nominalMatch: true,
    health: { status: 'OPTIMAL', latencyMs: 1.1, packetLossPct: 0.0, retriesPerSec: 0 },
  },
  {
    id: 'cable-sw-safety',
    sourceStationId: 'station-sw-01',
    sourcePortId: 'sw-p4',
    targetStationId: 'station-vfd-01',
    targetPortId: 'vfd-p2',
    protocol: 'SAFETY_PROFISAFE',
    cableLengthMeters: 6.0,
    nominalMatch: true,
    health: { status: 'OPTIMAL', latencyMs: 0.2, packetLossPct: 0.0, retriesPerSec: 0 },
  },
];
