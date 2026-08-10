import { readFile } from 'node:fs/promises';

export const ORACLE_STATUSES = Object.freeze(['passed', 'failed', 'not-run', 'unavailable']);

export const PINNED_ORACLES = Object.freeze({
  pyhanko: Object.freeze({ package: 'pyHanko', version: '0.29.0', commandContract: 'pyhanko sign validate --pretty-print' }),
  dss: Object.freeze({ product: 'EU DSS', version: '6.4.0', profile: 'PAdES Baseline B-B' }),
});

export async function loadSignedInteropContract(contractPath = new URL('./signed-interop-contract.json', import.meta.url)) {
  const contract = JSON.parse(await readFile(contractPath, 'utf8'));
  validateSignedInteropContract(contract);
  return contract;
}

export function validateSignedInteropContract(contract) {
  const errors = [];
  if (contract?.schema !== 'butter-paper/signed-interop-contract') errors.push('schema must be butter-paper/signed-interop-contract');
  if (contract?.version !== 1) errors.push('version must be 1');
  if (contract?.claimPolicy?.structuralInspection !== 'descriptive-only') errors.push('structural inspection must be descriptive-only');
  if (contract?.claimPolicy?.cryptographicValidity !== 'requires-independent-oracle') errors.push('cryptographic validity must require an independent oracle');
  if (contract?.claimPolicy?.signatureWidget !== 'not-proof-of-signature') errors.push('signature widgets must not be treated as proof');
  const flowIds = (contract?.flows ?? []).map((flow) => flow?.id);
  if (JSON.stringify(flowIds) !== JSON.stringify(['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H'])) errors.push('flows must contain deterministic A-H descriptors');
  for (const flow of contract?.flows ?? []) {
    if (!Array.isArray(flow.sequence) || flow.sequence.length < 1) errors.push(`flow ${flow?.id} sequence is required`);
    if (!Array.isArray(flow.requiredAxes) || flow.requiredAxes.length < 1) errors.push(`flow ${flow?.id} requiredAxes is required`);
    if (flow.status !== 'not-run') errors.push(`flow ${flow?.id} must start as not-run`);
  }
  for (const [name, pin] of Object.entries(PINNED_ORACLES)) {
    if (contract?.oraclePins?.[name]?.version !== pin.version) errors.push(`${name} pin must be ${pin.version}`);
  }
  if (errors.length) throw contractError('SIGNED_INTEROP_CONTRACT_INVALID', errors);
  return contract;
}

export function createOracleResult(tool, status = 'not-run', details = {}) {
  const pin = PINNED_ORACLES[tool];
  if (!pin) throw new Error(`Unknown signed evidence oracle: ${tool}`);
  if (!ORACLE_STATUSES.includes(status)) throw new Error(`Unknown oracle status: ${status}`);
  if ((status === 'not-run' || status === 'unavailable') && !details.reason) {
    throw new Error(`${tool} ${status} evidence requires a reason`);
  }
  return {
    schema: 'butter-paper/signed-oracle-result',
    version: 1,
    tool,
    pin,
    status,
    reason: details.reason ?? null,
    evidenceFiles: [...(details.evidenceFiles ?? [])],
    cryptographicValidity: status === 'passed' ? details.cryptographicValidity === true : null,
    capturedAt: details.capturedAt ?? null,
  };
}

export function validateOracleResult(result) {
  const errors = [];
  if (result?.schema !== 'butter-paper/signed-oracle-result') errors.push('oracle schema is invalid');
  if (!PINNED_ORACLES[result?.tool]) errors.push('oracle tool is not pinned');
  if (!ORACLE_STATUSES.includes(result?.status)) errors.push('oracle status is invalid');
  if (result?.pin?.version !== PINNED_ORACLES[result?.tool]?.version) errors.push('oracle pin does not match the contract');
  if (['not-run', 'unavailable'].includes(result?.status) && !result?.reason) errors.push('not-run/unavailable oracle results require a reason');
  if (result?.status !== 'passed' && result?.cryptographicValidity !== null) errors.push('non-passing oracle results cannot claim cryptographic validity');
  if (result?.status === 'passed' && result?.cryptographicValidity !== true) errors.push('passing oracle results must explicitly assert cryptographic validity');
  if (errors.length) throw contractError('SIGNED_ORACLE_RESULT_INVALID', errors);
  return result;
}

function contractError(code, errors) {
  const error = new Error(`${code}:\n${errors.map((item) => `- ${item}`).join('\n')}`);
  error.code = code;
  error.errors = errors;
  return error;
}
