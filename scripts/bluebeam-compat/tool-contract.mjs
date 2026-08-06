import { readFile } from 'node:fs/promises';

export async function loadToolContract(url = new URL('./tool-contract.json', import.meta.url)) {
  const contract = JSON.parse(await readFile(url, 'utf8'));
  validateToolContract(contract);
  return contract;
}

export function validateToolContract(contract) {
  if (contract?.schema !== 'butter-paper/bluebeam-tool-contract' || contract.version !== 1) throw new Error('Unsupported Bluebeam tool contract');
  const policy = contract.compatibilityPolicy;
  if (policy?.untouchedImported !== 'verbatim-native-components'
    || policy.editedImported !== 'canonical-native-replacement'
    || policy.deletedImported !== 'remove-all-logical-components'
    || policy.safeImprovements !== 'persist-only-standard-native-geometry') {
    throw new Error('Invalid compatibility policy');
  }
  if (!Array.isArray(policy.unsupportedEditedProperties) || policy.unsupportedEditedProperties.length === 0) {
    throw new Error('Compatibility policy must declare unsupported edited properties');
  }
  for (const exception of policy.unsupportedEditedProperties) {
    if (!Array.isArray(exception.fields) || exception.fields.length === 0
      || exception.fields.some((field) => typeof field !== 'string' || field.length === 0)
      || typeof exception.policy !== 'string' || exception.policy.length === 0
      || typeof exception.reason !== 'string' || exception.reason.length === 0) {
      throw new Error('Invalid unsupported edited property policy');
    }
  }
  const vocabulary = new Set(contract.operationVocabulary ?? []);
  const ids = new Set();
  for (const tool of contract.tools ?? []) {
    if (!tool.id || ids.has(tool.id)) throw new Error(`Invalid or duplicate tool id: ${tool.id}`);
    ids.add(tool.id);
    if (!['navigation', 'annotation'].includes(tool.kind)) throw new Error(`Invalid kind for ${tool.id}`);
    if (tool.componentCount !== tool.components?.length) throw new Error(`Component count mismatch for ${tool.id}`);
    if (tool.kind === 'navigation' && tool.componentCount !== 0) throw new Error(`Navigation tool ${tool.id} cannot emit annotations`);
    if (tool.kind === 'annotation' && tool.componentCount < 1) throw new Error(`Annotation tool ${tool.id} must emit a component`);
    const roles = new Set();
    for (const component of tool.components) {
      if (!component.role || roles.has(component.role) || !component.subtype) throw new Error(`Invalid component for ${tool.id}`);
      roles.add(component.role);
    }
    for (const operation of tool.operations ?? []) if (!vocabulary.has(operation)) throw new Error(`Unknown operation ${operation} for ${tool.id}`);
  }
  for (const required of ['select', 'pan', 'text-box', 'arrow', 'pen', 'highlight', 'cloud', 'cloud-plus', 'callout', 'image', 'snapshot', 'rectangle', 'ellipse', 'line', 'arc', 'polyline', 'polygon', 'dimension', 'length', 'polylength', 'area']) {
    if (!ids.has(required)) throw new Error(`Missing tool contract: ${required}`);
  }
  if (ids.size !== 21) throw new Error(`Expected exactly 21 current tools, received ${ids.size}`);
  return contract;
}

export function validateOperationResults(contract, results) {
  validateToolContract(contract);
  const indexed = new Map((results ?? []).map((result) => [`${result.tool}:${result.operation}`, result]));
  const missing = [];
  const failed = [];
  for (const tool of contract.tools) for (const operation of tool.operations) {
    const key = `${tool.id}:${operation}`;
    const result = indexed.get(key);
    if (!result) missing.push(key);
    else if (result.status !== 'passed') failed.push(key);
  }
  return { passed: missing.length === 0 && failed.length === 0, missing, failed };
}

export function expectedComponents(contract, toolIds) {
  validateToolContract(contract);
  const byId = new Map(contract.tools.map((tool) => [tool.id, tool]));
  return toolIds.flatMap((id) => {
    const tool = byId.get(id);
    if (!tool) throw new Error(`Unknown tool: ${id}`);
    return tool.components.map((component) => ({ tool: id, ...component }));
  });
}

export function validateInspectedComponents(contract, toolIds, inspection) {
  const expected = expectedComponents(contract, toolIds);
  const actual = (inspection?.pages ?? []).flatMap((page) => page.annotations ?? [])
    .filter((annotation) => annotation.subtype && annotation.subtype !== 'Popup' && annotation.subtype !== 'Link');
  const unused = new Set(actual.map((_, index) => index));
  const missing = [];
  for (const component of expected) {
    const match = [...unused].find((index) => componentMatchesInspection(component, actual[index]));
    if (match === undefined) {
      missing.push(`${component.tool}:${component.role}:${component.subtype}:${component.intent ?? 'none'}`);
    } else {
      unused.delete(match);
    }
  }
  const unexpected = [...unused].map((index) => {
    const annotation = actual[index];
    return `${annotation.subtype}:${annotation.intent ?? 'none'}:${annotation.name ?? index}`;
  });
  return { passed: missing.length === 0 && unexpected.length === 0, missing, unexpected };
}

function componentMatchesInspection(component, annotation) {
  return component.subtype === annotation.subtype
    && (component.intent ?? null) === (annotation.intent ?? null)
    && Boolean(component.measure) === Boolean(annotation.measure)
    && (!component.blendMode || component.blendMode === annotation.blendMode);
}
