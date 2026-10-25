import type { AppError } from "./bindings";

/** User-facing text for every backend error kind. The exhaustive switch makes a new kind a compile error until it is handled. */
export function describeError(error: AppError): string {
  switch (error.kind) {
    case "storageBusy":
      return "The library is busy. Try again in a moment.";
    case "storageFull":
      return "The disk is full, so the library cannot be updated.";
    case "storageCorrupt":
      return "The library database is damaged. Your original database file was left untouched.";
    case "storageTooNew":
      return `The library was created by a newer version of ScreamPad (schema ${error.database}, this version supports ${error.supported}). Please update the application.`;
    case "storageMigration":
      return "The library could not be upgraded. A backup was kept.";
    case "storageConstraint":
      return "The library rejected the change because it conflicts with existing data.";
    case "storageIo":
      return "The library storage is not accessible.";
    case "internal":
      return "Something went wrong inside the application.";
    default:
      return assertNever(error);
  }
}

function assertNever(value: never): never {
  throw new Error(`Unhandled error kind: ${JSON.stringify(value)}`);
}
