import { useMutation, useQueryClient } from '@tanstack/react-query';
import { createProfile, removeProfile, updateProfile } from '../../../ipc/commands';
import { projectKeys } from '../query-keys';
import type { ProfileDraft, RemoveProfileRequest, UpdateProfileRequest } from '../../../ipc/types';
import { invalidateApplicationState } from '../../runtime/useApplicationStateEvents';
export function useProfileMutations() {
  const client = useQueryClient();
  const changed = () => invalidateApplicationState(client, ['profiles', 'discovery', 'diagnostics']);
  const create = useMutation({ mutationFn: (draft: ProfileDraft) => createProfile(draft), onSuccess: changed });
  const update = useMutation({ mutationFn: (request: UpdateProfileRequest) => updateProfile(request), onSuccess: profile => { changed(); void client.invalidateQueries({ queryKey: projectKeys.detail(profile.id) }); void client.invalidateQueries({ queryKey: projectKeys.definition(profile.id) }); } });
  const remove = useMutation({ mutationFn: (request: RemoveProfileRequest) => removeProfile(request), onSuccess: (_data, request) => { changed(); client.removeQueries({ queryKey: projectKeys.detail(request.profileId) }); client.removeQueries({ queryKey: projectKeys.status(request.profileId) }); client.removeQueries({ queryKey: projectKeys.definition(request.profileId) }); } });
  return { create, update, remove };
}
